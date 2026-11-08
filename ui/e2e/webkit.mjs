// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Smoke test of the *real* desktop app (Rust backend + WebKitGTK on Linux)
// over WebDriver, using native input so events really originate inside the
// sandboxed chapter iframe.
//
//   tauri-driver &      (with DISPLAY set and XDG_DATA_HOME pointing at a seeded library)
//   node e2e/webkit.mjs <app-binary> [screenshot-dir]
//
// Docs: https://v2.tauri.app/develop/tests/webdriver/

import { writeFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const app = resolve(process.argv[2]);
const shots = resolve(process.argv[3] ?? "e2e-shots-webkit");
mkdirSync(shots, { recursive: true });
const WD = "http://127.0.0.1:4444";

// ----- minimal WebDriver client ---------------------------------------------
async function wd(method, path, body) {
  const r = await fetch(WD + path, { method, headers: { "Content-Type": "application/json" }, body: body ? JSON.stringify(body) : undefined });
  const j = await r.json();
  if (j.value && j.value.error) throw new Error(`${path}: ${j.value.error} ${j.value.message}`);
  return j.value;
}
const { sessionId } = await wd("POST", "/session", { capabilities: { alwaysMatch: { "tauri:options": { application: app } } } });
const S = `/session/${sessionId}`;
const exec = (fn, ...args) => wd("POST", `${S}/execute/async`, {
  script: `const done = arguments[arguments.length - 1]; Promise.resolve((${fn}).apply(null, Array.from(arguments).slice(0, -1))).then(done, e => done({ __error: String(e) }));`,
  args,
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (name) => writeFileSync(join(shots, name), Buffer.from(await wd("GET", `${S}/screenshot`), "base64"));
const keys = (...ks) => wd("POST", `${S}/actions`, { actions: [{ type: "key", id: "kb", actions: ks.flatMap((k) => [{ type: "keyDown", value: k }, { type: "keyUp", value: k }]) }] });
const drag = (x1, y1, x2, y2) => wd("POST", `${S}/actions`, { actions: [{ type: "pointer", id: "m", parameters: { pointerType: "mouse" }, actions: [
  { type: "pointerMove", x: Math.round(x1), y: Math.round(y1), duration: 0 }, { type: "pointerDown", button: 0 },
  { type: "pointerMove", x: Math.round(x2), y: Math.round(y2), duration: 200 }, { type: "pointerUp", button: 0 }] }] });
const click = (x, y) => drag(x, y, x, y);

let failures = 0;
const check = (cond, label, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${label}${extra ? `  (${extra})` : ""}`);
  if (!cond) failures++;
};
const invoke = (cmd, args) => exec((c, a) => window.__TAURI_INTERNALS__.invoke(c, a), cmd, args ?? {});
const pos = () => exec(() => document.querySelector(".pos")?.innerText ?? "");

try {
  // STEP 1: library from the real SQLite database.
  await sleep(2500);
  const books = await invoke("list_books", { query: { sort: "title", descending: false } });
  check(Array.isArray(books) && books.length === 2, "real backend lists the seeded books", books.map?.((b) => b.title).join(", "));
  await shot("w1-library.png");

  // STEP 2: open the EPUB by clicking its card.
  const lantern = books.find((b) => b.title === "The Lantern Harbor");
  await exec((t) => [...document.querySelectorAll(".card")].find((c) => c.innerText.includes(t)).click(), lantern.title);
  await sleep(2500);
  const state = await exec(() => {
    const d = document.querySelector(".host iframe")?.contentDocument;
    const img = d?.querySelector("img");
    return { blocks: d?.querySelectorAll("[data-jrb]").length ?? 0, img: img ? img.naturalWidth : -1, css: d ? getComputedStyle(d.querySelector("h1")).textAlign : "" };
  });
  check(state.blocks > 10, "chapter renders in WebKitGTK", `blocks=${state.blocks}`);
  check(state.img > 0, "image loads through the justbook:// protocol", `naturalWidth=${state.img}`);
  check(state.css === "center", "book stylesheet loads through the protocol", `h1 text-align=${state.css}`);
  check(/Page 1 of \d+/.test(await pos()), "paginated, page 1", await pos());
  await shot("w2-reader.png");

  // STEP 3: native key presses (focus is in the main document).
  await keys("", "", ""); // ArrowRight ×3
  await sleep(1200);
  check(/Page 4 of/.test(await pos()), "ArrowRight ×3 → page 4", await pos());
  const p1 = (await invoke("open_book", { id: lantern.id })).progress?.locator;
  check(!!p1, "progress persisted in SQLite", p1);

  // STEP 4: pages → scroll → pages keeps the locator.
  await exec(() => document.querySelector('button[title="Switch to scrolling"]').click());
  await sleep(1800);
  await shot("w3-scroll.png");
  const p2 = (await invoke("open_book", { id: lantern.id })).progress?.locator;
  await exec(() => document.querySelector('button[title="Switch to pages"]').click());
  await sleep(1800);
  const p3 = (await invoke("open_book", { id: lantern.id })).progress?.locator;
  check(p1 === p2 && p2 === p3, "layout switches keep the exact locator", `${p1} → ${p2} → ${p3}`);
  check(/Page 4 of/.test(await pos()), "…back on page 4", await pos());

  // STEP 5: native drag-selection inside the sandboxed iframe → highlight.
  const box = await exec(() => {
    const f = document.querySelector(".host iframe");
    const fr = f.getBoundingClientRect();
    const d = f.contentDocument;
    const p = [...d.querySelectorAll("p[data-jrb]")].find((el) => { const r = el.getBoundingClientRect(); return r.left >= 0 && r.right <= d.defaultView.innerWidth && r.top > 20; });
    const r = p.getClientRects()[0];
    return { x: fr.left + r.left + 40, y: fr.top + r.top + r.height / 2, w: Math.min(220, r.width - 60) };
  });
  await drag(box.x, box.y, box.x + box.w, box.y);
  await sleep(800);
  const menu = await exec(() => !!document.querySelector(".menu"));
  check(menu, "native drag-select inside the iframe opens the highlight menu");
  if (menu) {
    const sw = await exec(() => { const r = document.querySelector(".menu .swatch").getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; });
    await click(sw.x, sw.y);
    await sleep(1000);
    const anns = await invoke("list_annotations", { bookId: lantern.id });
    check(anns.length === 1 && anns[0].kind === "highlight", "highlight stored in SQLite", anns[0] && `${anns[0].start}..${anns[0].end} "${anns[0].quote}"`);
    const marks = await exec(() => document.querySelector(".host iframe").contentDocument.querySelectorAll("mark.jr-hl").length);
    check(marks > 0, "highlight painted");
  }
  await shot("w4-highlight.png");

  // STEP 6: native click on the right third turns the page (tap zones).
  const before = await pos();
  const fr = await exec(() => { const r = document.querySelector(".host").getBoundingClientRect(); return { x: r.right - 60, y: r.top + r.height / 2 }; });
  await click(fr.x, fr.y);
  await sleep(1000);
  check((await pos()) !== before, "clicking the right edge of the page turns it", `${before} → ${await pos()}`);
} catch (e) {
  failures++;
  console.log("FAIL  scenario aborted:", e.message);
  await shot("wz-failure.png").catch(() => {});
} finally {
  await wd("DELETE", S).catch(() => {});
}
console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
