//! The pure reducer from [`SurfaceEvent`]s to [`SurfaceState`].

use crate::model::{Entity, EntityKind, Lifecycle, SurfaceEvent};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// A runtime restart observed by this surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EpochMark {
    pub epoch: u64,
    pub at_ms: u64,
    pub reason: String,
    /// In-flight entities this restart settled to `unknown`.
    pub settled: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Tracked {
    pub(crate) entity: Entity,
    /// First-seen order; the stable sibling order of the card stream.
    pub(crate) seq: u64,
    /// Cursor of the last change applied to this entity.
    pub(crate) last_cursor: u64,
    pub(crate) last_event_at_ms: u64,
    /// The surface itself settled this entity to `unknown` (restart or
    /// disconnect) rather than the runtime reporting the outcome.
    pub(crate) settled_locally: bool,
    /// A definitive outcome later replaced a local `unknown` settlement.
    pub(crate) reconciled: bool,
    /// Execution attempts on this card: a terminal entity resumed by a newer
    /// runtime epoch starts attempt 2, and so on.
    pub(crate) attempt: u32,
}

/// Reduced surface state. Apply events in delivery order with
/// [`SurfaceState::apply`]; render with [`SurfaceState::project`].
#[derive(Clone, Debug, Default)]
pub struct SurfaceState {
    pub(crate) epoch: u64,
    pub(crate) cursor: u64,
    pub(crate) last_event_at_ms: Option<u64>,
    pub(crate) last_heartbeat_at_ms: Option<u64>,
    pub(crate) context_pct: Option<u32>,
    pub(crate) entities: HashMap<String, Tracked>,
    next_seq: u64,
    pub(crate) replays_ignored: u64,
    pub(crate) regressions_ignored: u64,
    pub(crate) restarts: Vec<EpochMark>,
    /// Whether any runtime epoch has been observed; the first observation is
    /// a baseline, not a restart.
    epoch_known: bool,
    pub(crate) disconnected_at_ms: Option<u64>,
}

impl SurfaceState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current runtime epoch.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Highest applied cursor; resume deltas after it on reconnect.
    #[must_use]
    pub const fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Runtime restarts observed so far.
    #[must_use]
    pub fn restarts(&self) -> &[EpochMark] {
        &self.restarts
    }

    #[must_use]
    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id).map(|tracked| &tracked.entity)
    }

    /// Entities in first-seen order.
    #[must_use]
    pub fn entities(&self) -> Vec<&Entity> {
        self.ordered()
            .into_iter()
            .map(|tracked| &tracked.entity)
            .collect()
    }

    pub(crate) fn ordered(&self) -> Vec<&Tracked> {
        let mut tracked: Vec<_> = self.entities.values().collect();
        tracked.sort_by_key(|tracked| tracked.seq);
        tracked
    }

    /// Apply one event. Replays (cursor at or below the applied cursor) and
    /// lifecycle regressions are counted and otherwise ignored.
    pub fn apply(&mut self, event: &SurfaceEvent) {
        match event {
            SurfaceEvent::Snapshot {
                epoch,
                cursor,
                at_ms,
                entities,
            } => self.apply_snapshot(*epoch, *cursor, *at_ms, entities),
            SurfaceEvent::Heartbeat {
                at_ms, context_pct, ..
            } => {
                if self.last_heartbeat_at_ms.is_none_or(|last| *at_ms >= last) {
                    self.last_heartbeat_at_ms = Some(*at_ms);
                    if context_pct.is_some() {
                        self.context_pct = *context_pct;
                    }
                }
            }
            SurfaceEvent::Connection { connected, at_ms } => {
                self.disconnected_at_ms = if *connected {
                    None
                } else {
                    Some(self.disconnected_at_ms.unwrap_or(*at_ms))
                };
            }
            _ if event.cursor().is_some_and(|cursor| cursor <= self.cursor) => {
                self.replays_ignored += 1;
            }
            SurfaceEvent::Upsert {
                cursor,
                at_ms,
                entity,
            } => {
                self.upsert(entity, *cursor, *at_ms);
                self.advance(*cursor, *at_ms);
            }
            SurfaceEvent::RuntimeEpoch {
                epoch,
                cursor,
                at_ms,
                reason,
            } => {
                self.settle_epoch(*epoch, *cursor, *at_ms, reason);
                self.advance(*cursor, *at_ms);
            }
        }
    }

    const fn advance(&mut self, cursor: u64, at_ms: u64) {
        self.cursor = cursor;
        self.last_event_at_ms = Some(at_ms);
    }

    fn apply_snapshot(&mut self, epoch: u64, cursor: u64, at_ms: u64, entities: &[Entity]) {
        if cursor < self.cursor {
            self.replays_ignored += 1;
            return;
        }
        if self.epoch_known && epoch > self.epoch {
            self.settle_epoch(epoch, cursor, at_ms, "runtime restarted while disconnected");
        }
        let open: HashSet<&str> = entities.iter().map(|entity| entity.id.as_str()).collect();
        for tracked in self.entities.values_mut() {
            let entity = &mut tracked.entity;
            // Snapshots list runtime lifecycle entities; surface-local cards
            // (workspace, hands, status) are not theirs to close.
            if entity.kind().settles_on_epoch()
                && !entity.state.is_terminal()
                && !open.contains(entity.id.as_str())
            {
                entity.state = Lifecycle::Unknown;
                entity.reason = Some("closed while disconnected".to_owned());
                entity.updated_at_ms = at_ms;
                tracked.settled_locally = true;
                tracked.last_cursor = cursor;
                tracked.last_event_at_ms = at_ms;
            }
        }
        for entity in entities {
            self.upsert(entity, cursor, at_ms);
        }
        self.epoch = self.epoch.max(epoch);
        self.epoch_known = true;
        self.advance(cursor, at_ms);
    }

    fn upsert(&mut self, entity: &Entity, cursor: u64, at_ms: u64) {
        let Some(tracked) = self.entities.get_mut(&entity.id) else {
            self.entities.insert(
                entity.id.clone(),
                Tracked {
                    entity: entity.clone(),
                    seq: self.next_seq,
                    last_cursor: cursor,
                    last_event_at_ms: at_ms,
                    settled_locally: false,
                    reconciled: false,
                    attempt: 1,
                },
            );
            self.next_seq += 1;
            return;
        };
        let previous = tracked.entity.state;
        let next = entity.state;
        let resumed_in_new_epoch = entity.epoch > tracked.entity.epoch;
        if previous.is_terminal() && !next.is_terminal() && !resumed_in_new_epoch {
            self.regressions_ignored += 1;
            return;
        }
        if previous.is_definitive() && next == Lifecycle::Unknown {
            self.regressions_ignored += 1;
            return;
        }
        if previous.is_terminal() && !next.is_terminal() {
            tracked.attempt += 1;
        }
        let reconciles = tracked.settled_locally && next.is_definitive();
        if reconciles {
            tracked.reconciled = true;
        }
        if !next.is_terminal() || next.is_definitive() {
            tracked.settled_locally = false;
        }
        tracked.entity = entity.clone();
        tracked.last_cursor = cursor;
        tracked.last_event_at_ms = at_ms;
        if next.is_terminal() && !previous.is_terminal() {
            self.settle_orphaned_tool_calls(&entity.id, cursor, at_ms);
        }
    }

    /// A tool call cannot outlive the turn, run, agent or Code Mode cell that
    /// issued it. When the owner ends first, its still-open tool calls (and
    /// their nested calls) settle to `unknown`: the owner's terminal event is
    /// explicit, the tool's own outcome was never reported.
    fn settle_orphaned_tool_calls(&mut self, owner: &str, cursor: u64, at_ms: u64) {
        let Some(kind) = self
            .entities
            .get(owner)
            .map(|tracked| tracked.entity.kind())
        else {
            return;
        };
        if !kind.settles_on_epoch() || kind == EntityKind::ProcessSession {
            return;
        }
        let mut owners = vec![owner.to_owned()];
        while let Some(owner) = owners.pop() {
            for tracked in self.entities.values_mut() {
                let entity = &mut tracked.entity;
                if entity.kind() != EntityKind::ToolCall
                    || entity.parent_id.as_deref() != Some(owner.as_str())
                {
                    continue;
                }
                owners.push(entity.id.clone());
                if entity.state.is_terminal() {
                    continue;
                }
                entity.state = Lifecycle::Unknown;
                entity.reason = Some("owner ended without a tool result".to_owned());
                entity.updated_at_ms = at_ms;
                tracked.settled_locally = true;
                tracked.last_cursor = cursor;
                tracked.last_event_at_ms = at_ms;
            }
        }
    }

    fn settle_epoch(&mut self, epoch: u64, cursor: u64, at_ms: u64, reason: &str) {
        if epoch <= self.epoch {
            self.replays_ignored += 1;
            return;
        }
        let mut settled = 0;
        for tracked in self.entities.values_mut() {
            let entity = &mut tracked.entity;
            if entity.kind().settles_on_epoch()
                && !entity.state.is_terminal()
                && entity.epoch < epoch
            {
                entity.state = Lifecycle::Unknown;
                entity.reason = Some(reason.to_owned());
                entity.updated_at_ms = at_ms;
                tracked.settled_locally = true;
                tracked.last_cursor = cursor;
                tracked.last_event_at_ms = at_ms;
                settled += 1;
            }
        }
        if self.epoch_known || settled > 0 {
            self.restarts.push(EpochMark {
                epoch,
                at_ms,
                reason: reason.to_owned(),
                settled,
            });
        }
        self.epoch = epoch;
        self.epoch_known = true;
    }
}
