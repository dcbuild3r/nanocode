#!/usr/bin/env node
// Render every shared fixture step through the JavaScript reducer into
// standalone HTML snapshots: node scripts/render-fixtures.mjs <out-dir>
// [--chrome <headless-chrome-binary>] adds a PNG screenshot per snapshot.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { SurfaceState } from "../index.mjs";
import { renderCardStreamHtml } from "../html.mjs";

const args = process.argv.slice(2);
const outDir = resolve(args[0] ?? "output/surface-state-web");
const chromeIndex = args.indexOf("--chrome");
const chrome = chromeIndex === -1 ? undefined : args[chromeIndex + 1];
const fixtures = new URL("../../../crates/nanocodex-surface-state/fixtures/", import.meta.url);
const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

mkdirSync(outDir, { recursive: true });
const names = readdirSync(fixtures)
  .filter((file) => file.endsWith(".json") && !file.endsWith(".cards.json"))
  .map((file) => file.slice(0, -".json".length))
  .sort();

for (const name of names) {
  const fixture = JSON.parse(readFileSync(new URL(`${name}.json`, fixtures), "utf8"));
  const state = new SurfaceState();
  fixture.steps.forEach((step, index) => {
    for (const event of step.events) state.apply(event);
    const projection = state.project(step.now_ms);
    const html = [
      "<!doctype html><html><head><meta charset=utf-8>",
      `<title>${name} — ${step.label}</title><style>${css} body{margin:0;background:#0f1115}</style></head>`,
      `<body><h1 style="margin:0;padding:12px 12px 0;color:#8b93a7;font:600 13px system-ui">${name} · step ${index + 1}: ${step.label}</h1>`,
      renderCardStreamHtml(projection),
      "</body></html>",
    ].join("\n");
    const base = join(outDir, `${name}-${index + 1}`);
    writeFileSync(`${base}.html`, html);
    if (chrome) {
      execFileSync(chrome, [
        "--headless",
        "--no-sandbox",
        "--hide-scrollbars",
        "--window-size=900,1800",
        `--screenshot=${base}.png`,
        `file://${base}.html`,
      ], { stdio: "ignore" });
    }
    console.log(`${base}.html${chrome ? ` + .png` : ""} (${projection.cards.length} cards)`);
  });
}
