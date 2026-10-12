# nanocodex-surface-state (JavaScript)

JavaScript mirror of `crates/nanocodex-surface-state`: the same reducer and card
projection, for web surfaces.

```js
import { SurfaceState } from "nanocodex-surface-state";
import { renderCardStreamHtml } from "nanocodex-surface-state/html";
import "nanocodex-surface-state/styles.css";

const state = new SurfaceState();
for (const event of events) state.apply(event);
element.innerHTML = renderCardStreamHtml(state.project(Date.now()));
```

`pnpm test` checks this implementation against the Rust golden projections in
`crates/nanocodex-surface-state/fixtures`. `pnpm render:fixtures <dir> [--chrome <binary>]`
writes an HTML snapshot (and optional headless-Chromium PNG) per fixture step.
