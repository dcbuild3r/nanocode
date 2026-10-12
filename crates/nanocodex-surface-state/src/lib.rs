//! Shared surface state for every Nanocodex surface.
//!
//! Surfaces (TUI, web, native apps) must not reconstruct lifecycle by
//! inference from transcript events. Instead the runtime emits explicit
//! entity lifecycle events ([`SurfaceEvent`]); [`SurfaceState`] is a pure,
//! deterministic reducer over them, and [`SurfaceState::project`] yields the
//! [`Projection`] of cards that each surface renders verbatim.
//!
//! Guarantees enforced by the reducer:
//!
//! - **Cursor fencing.** Deltas at or below the applied cursor are replays and
//!   are counted, never re-applied, so replayed completions cannot resurrect
//!   or duplicate cards after a reconnect.
//! - **Sticky terminal states.** A completed, failed or cancelled entity never
//!   returns to admitted/running. `unknown` may be reconciled by a later
//!   definitive terminal state, which is labelled as reconciled.
//! - **Epoch settlement.** A `runtime_epoch` event settles every in-flight
//!   turn, run, agent, tool call and process session from an older epoch to
//!   `unknown` ("runtime restarted"). Nothing keeps ticking after a restart.
//! - **Snapshot authority.** A snapshot lists the open entities. A locally
//!   open entity absent from it is settled to `unknown` ("closed while
//!   disconnected") rather than guessed to have succeeded.
//!
//! The projection is a pure function of state and the caller-supplied clock,
//! so the JavaScript mirror reproduces it exactly; the shared fixtures in
//! `fixtures/` are checked against one golden projection by both
//! implementations.

pub mod agent_events;
mod model;
mod project;
mod reduce;

pub use model::{
    Body, Entity, EntityKind, Lifecycle, Provenance, Resources, StatusItem, StatusSection, Summary,
    SurfaceEvent,
};
pub use project::{Badge, Card, CardLine, Liveness, Projection, Tone, format_duration};
pub use reduce::{EpochMark, SurfaceState};

/// Hand resource observations older than this are flagged as stale.
pub const STALE_RESOURCES_MS: u64 = 5 * 60 * 1000;
/// A running entity with no event for this long is flagged as quiet.
pub const QUIET_MS: u64 = 60 * 1000;
