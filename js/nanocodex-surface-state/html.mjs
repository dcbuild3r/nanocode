// DOM-free HTML rendering of a surface-state projection. The web surface and
// static snapshots share this markup; React consumers may render the same
// projection fields with their own components.

const escape = (value) =>
  String(value).replace(/[&<>"']/g, (char) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char],
  );

const KIND_LABEL = {
  agent: "Agent",
  turn: "Turn",
  tool_call: "Tool",
  process_session: "Process",
  run: "Run",
  hand: "Hand",
  brain: "Brain",
  decision: "Decision",
  status: "Status",
};

/** Render one card as an `<article>`. */
export function renderCardHtml(card) {
  const badges = card.badges
    .map((badge) => `<span class="ncs-badge ncs-tone-${badge.tone}">${escape(badge.text)}</span>`)
    .join("");
  const lines = card.lines
    .map((line) => `<li class="ncs-line ncs-tone-${line.tone}">${escape(line.text)}</li>`)
    .join("");
  const subject = card.subject === undefined ? "" : `<span class="ncs-subject">${escape(card.subject)}</span>`;
  return [
    `<article class="ncs-card ncs-kind-${card.kind} ncs-tone-${card.tone}${card.pinned ? " ncs-pinned" : ""}"`,
    ` data-card-id="${escape(card.id)}" data-state="${card.state}" style="--ncs-depth:${card.depth}">`,
    `<header><span class="ncs-kind">${KIND_LABEL[card.kind] ?? escape(card.kind)}</span>`,
    `<span class="ncs-title">${escape(card.title)}</span>${subject}`,
    `<span class="ncs-state ncs-tone-${card.tone}">${escape(card.state_label)}</span></header>`,
    badges ? `<div class="ncs-badges">${badges}</div>` : "",
    lines ? `<ul class="ncs-lines">${lines}</ul>` : "",
    "</article>",
  ].join("");
}

/** Render a whole projection: card stream plus liveness footer. */
export function renderCardStreamHtml(projection) {
  const cards = projection.cards.map(renderCardHtml).join("\n");
  const liveness = projection.liveness;
  return [
    '<section class="ncs-stream" aria-label="Surface state">',
    cards,
    `<footer class="ncs-liveness ncs-tone-${liveness.tone}">${escape(liveness.label)}</footer>`,
    "</section>",
  ].join("\n");
}
