// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// "Open with justRead" on Linux/Windows: the app is started with a book file
// as its argument (what a file manager does). It must import the book and
// open it in the reader.
//
//   tauri-driver &   (with XDG_DATA_HOME pointing at an empty folder)
//   node e2e/openwith.mjs <app-binary> <book-file>

import { resolve } from "node:path";

const WD = "http://127.0.0.1:4444";
async function wd(method, path, body) {
  const r = await fetch(WD + path, { method, headers: { "Content-Type": "application/json" }, body: body ? JSON.stringify(body) : undefined });
  const j = await r.json();
  if (!r.ok && j.value && j.value.error) throw new Error(`${path}: ${j.value.error} ${j.value.message}`);
  return j.value;
}
const book = resolve(process.argv[3]);
const { sessionId } = await wd("POST", "/session", {
  capabilities: { alwaysMatch: { "tauri:options": { application: resolve(process.argv[2]), args: [book] } } },
});
const S = `/session/${sessionId}`;
const exec = (src) => wd("POST", `${S}/execute/async`, { script: `const done = arguments[0]; (async () => { ${src} })().then(done, e => done("ERR " + e));`, args: [] });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
const check = (c, l, x = "") => { console.log(`${c ? "PASS" : "FAIL"}  ${l}${x ? `  (${x})` : ""}`); if (!c) failures++; };
try {
  await sleep(4000);
  const books = await exec(`return (await window.__TAURI_INTERNALS__.invoke("list_books", { query: { sort: "title", descending: false } })).map(b => b.title);`);
  check(Array.isArray(books) && books.length === 1, "the file passed by the OS was imported", JSON.stringify(books));
  const reader = await exec(`return { iframe: !!document.querySelector(".host iframe"), title: document.querySelector(".book")?.innerText ?? null };`);
  check(reader.iframe && reader.title === books[0], "…and opened straight in the reader", JSON.stringify(reader));
} catch (e) {
  failures++;
  console.log("FAIL  aborted:", e.message);
} finally {
  await wd("DELETE", S).catch(() => {});
}
console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
