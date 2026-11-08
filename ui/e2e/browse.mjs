// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// justBrowse end-to-end test against the real app (WebKitGTK, Linux).
//
//   JUSTREAD_BROWSE_ALLOW_LOOPBACK=1 tauri-driver &      (debug build only)
//   node e2e/browse.mjs <app-binary> <sample.epub> [screenshot-dir]
//
// A local web server plays "the internet". Its page tries to escape the
// sandbox (Tauri commands, events, dialogs, justbook:// files, storage)
// and reports the outcome back to the server, so isolation is verified from
// *inside* the web content. WebDriver drives only the toolbar webview, like
// a user would.

import http from "node:http";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const app = resolve(process.argv[2]);
const epub = readFileSync(resolve(process.argv[3]));
const shots = resolve(process.argv[4] ?? "e2e-shots-browse");
mkdirSync(shots, { recursive: true });

// ----- the fake web --------------------------------------------------------
const reports = [];
const hits = [];
const PAGE = `<!doctype html><html><head><title>Free Books Test</title></head><body>
<h1>Free Books Test</h1>
<p><a href="/book.epub">Download a free book</a></p>
<img src="https://doubleclick.net/pixel.gif" alt=""><img src="https://www.google-analytics.com/collect" alt="">
<script>
(async () => {
  const r = {};
  const T = window.__TAURI_INTERNALS__;
  r.internals = typeof T;
  const timeout = (p) => Promise.race([p, new Promise((res) => setTimeout(() => res("timeout"), 3000))]);
  async function attempt(cmd, args) {
    if (!T || !T.invoke) return "no-ipc";
    try { await timeout(T.invoke(cmd, args)); return "ALLOWED"; } catch (e) { return "denied: " + String(e).slice(0, 100); }
  }
  r.list_books = await attempt("list_books", { query: { sort: "title", descending: false } });
  r.open_browser = await attempt("open_browser");
  r.browse_navigate = await attempt("browse_navigate", { input: "https://evil.example" });
  r.set_setting = await attempt("set_setting", { key: "reader", value: {} });
  r.event_emit = await attempt("plugin:event|emit", { event: "library-changed", payload: null });
  r.dialog = await attempt("plugin:dialog|open", { options: {} });
  try { const res = await fetch("justbook://localhost/00000000-0000-4000-8000-000000000000/cover.jpg"); r.justbook = "status " + res.status; } catch (e) { r.justbook = "error"; }
  try { r.localStorage = localStorage.length; } catch (e) { r.localStorage = "error"; }
  await fetch("/report", { method: "POST", body: JSON.stringify(r) });
})();
</script></body></html>`;
const server = http.createServer((req, res) => {
  hits.push(req.url);
  if (req.url === "/report") {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => { reports.push(JSON.parse(body)); res.end("ok"); });
    return;
  }
  if (req.url === "/book.epub") {
    res.writeHead(200, { "Content-Type": "application/epub+zip", "Content-Disposition": 'attachment; filename="lantern.epub"' });
    return res.end(epub);
  }
  // Third-party cookie probe: served on "localhost", embedded by the 127.0.0.1 page.
  if (req.url === "/tp-frame") {
    res.writeHead(200, { "Content-Type": "text/html", "Set-Cookie": "tp=1; Path=/" });
    return res.end(`<script>document.cookie = "tpjs=1; path=/";
      setTimeout(() => fetch("/tp-check").then(() => {}), 300);</script>`);
  }
  // First-party control: proves cookies work at all in this setup.
  if (req.url === "/fp-set") {
    res.writeHead(200, { "Content-Type": "text/html", "Set-Cookie": "fp=1; Path=/" });
    return res.end(`<script>setTimeout(() => fetch("/fp-check"), 300);</script>`);
  }
  if (req.url === "/fp-check") {
    reports.push({ firstPartyCookie: req.headers.cookie ?? "" });
    return res.end("ok");
  }
  if (req.url === "/tp-check") {
    reports.push({ thirdPartyCookie: req.headers.cookie ?? "" });
    return res.end("ok");
  }
  if (req.url === "/tp-page") {
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(`<h1>Embeds a third party</h1><iframe src="http://127.0.0.2:${server.address().port}/tp-frame"></iframe>`);
  }
  if (req.url === "/notes.bin") {
    res.writeHead(200, { "Content-Type": "application/octet-stream", "Content-Disposition": 'attachment; filename="notes.bin"' });
    return res.end(Buffer.from([0, 1, 2, 3, 4, 5, 6, 7]));
  }
  res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  res.end(PAGE);
});
// All loopback addresses (127.0.0.2 serves as a *different site* for cookie tests).
await new Promise((r) => server.listen(0, "0.0.0.0", r));
const SITE = `http://127.0.0.1:${server.address().port}`;

// ----- WebDriver helpers ---------------------------------------------------
const WD = "http://127.0.0.1:4444";
async function wd(method, path, body) {
  const r = await fetch(WD + path, { method, headers: { "Content-Type": "application/json" }, body: body ? JSON.stringify(body) : undefined });
  const j = await r.json();
  // WebDriver errors are non-2xx responses with value.error set.
  if (!r.ok && j.value && j.value.error) throw new Error(`${path}: ${j.value.error} ${j.value.message}`);
  return j.value;
}
const { sessionId } = await wd("POST", "/session", { capabilities: { alwaysMatch: { "tauri:options": { application: app } } } });
const S = `/session/${sessionId}`;
const exec = (fn, ...args) => wd("POST", `${S}/execute/async`, {
  script: `const done = arguments[arguments.length - 1]; Promise.resolve((${fn}).apply(null, Array.from(arguments).slice(0, -1))).then(done, e => done({ __error: String(e) }));`,
  args,
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 10000) => { const t = Date.now(); while (Date.now() - t < ms) { const v = await fn(); if (v) return v; await sleep(250); } return null; };
const shot = async (name) => writeFileSync(join(shots, name), Buffer.from(await wd("GET", `${S}/screenshot`), "base64"));
let failures = 0;
const check = (cond, label, extra = "") => { console.log(`${cond ? "PASS" : "FAIL"}  ${label}${extra ? `  (${extra})` : ""}`); if (!cond) failures++; };
/** Type into the toolbar address bar and press Enter. */
const typeAddress = (text) => exec((t) => {
  const i = document.querySelector(".address");
  i.focus();
  i.value = t;
  i.dispatchEvent(new Event("input", { bubbles: true }));
  i.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
}, text);
const toolbar = () => exec(() => ({
  address: document.querySelector(".address").value,
  warning: document.querySelector(".warning")?.innerText ?? null,
  shield: document.querySelector(".shield")?.innerText ?? "",
  status: document.querySelector(".status")?.innerText ?? "",
  errorText: document.querySelector(".err")?.innerText ?? "",
}));

try {
  // STEP 1: open justBrowse from the reader and switch WebDriver to its toolbar.
  await sleep(2500);
  const main = (await wd("GET", `${S}/window/handles`))[0];
  const before = (await exec(() => window.__TAURI_INTERNALS__.invoke("list_books", { query: { sort: "title", descending: false } }))).length;
  await exec(() => window.__TAURI_INTERNALS__.invoke("open_browser"));
  const browse = await until(async () => (await wd("GET", `${S}/window/handles`)).find((h) => h !== main));
  await wd("POST", `${S}/window`, { handle: browse });
  check((await wd("GET", `${S}/url`)).endsWith("/browse.html"), "justBrowse opens with its own toolbar webview");
  await sleep(1500);
  // The toolbar webview must be just the toolbar (GTK initially splits child webviews evenly).
  const tbHeight = await exec(() => window.innerHeight);
  check(tbHeight >= 90 && tbHeight <= 110, "toolbar webview is toolbar-sized, the page gets the rest", `${tbHeight}px`);

  // STEP 2: plain HTTP → warning first, page only after "Continue anyway".
  await typeAddress(SITE + "/");
  const warned = await until(async () => (await toolbar()).warning);
  check(!!warned && hits.length === 0, "HTTP site is not loaded before the user accepts the warning", warned);
  await shot("b1-http-warning.png");
  await exec(() => [...document.querySelectorAll(".warning button")].find((b) => b.innerText.includes("Continue")).click());
  const report = await until(() => reports[0], 15000);
  check(!!report, "page loads after accepting the warning and runs its probes");

  // STEP 3: isolation, as observed from inside the web content.
  if (report) {
    for (const k of ["list_books", "open_browser", "browse_navigate", "set_setting", "event_emit", "dialog"]) {
      check(report[k] !== "ALLOWED", `web content cannot call ${k}`, report[k]);
    }
    check(report.justbook !== "status 200", "web content cannot read justbook:// book files", report.justbook);
    check(report.localStorage === 0, "web content starts with empty, private storage", String(report.localStorage));
  }

  // STEP 4: trackers on the page were blocked by the proxy.
  const shield = await until(async () => { const s = (await toolbar()).shield; return /\d+/.test(s) && Number(s.match(/\d+/)[0]) >= 2 ? s : null; });
  check(!!shield, "tracker requests are blocked and counted", shield ?? (await toolbar()).shield);
  await shot("b2-page.png");
  // WebDriver only captures the toolbar webview; grab the whole X11 screen too (if available).
  try { execFileSync("import", ["-window", "root", join(shots, "b2-window.png")]); } catch { /* no X11 tools */ }

  // STEP 5: downloading an EPUB imports it into the library.
  await typeAddress(SITE + "/book.epub");
  const done = await until(async () => { const s = (await toolbar()).status; return /added to your library|already in your library/.test(s) ? s : null; }, 20000);
  check(!!done, "downloaded EPUB is imported", done ?? (await toolbar()).status);
  await wd("POST", `${S}/window`, { handle: main });
  const after = (await exec(() => window.__TAURI_INTERNALS__.invoke("list_books", { query: { sort: "title", descending: false } }))).length;
  check(after === before + 1 || /already/.test(done ?? ""), "the reader's library now contains it", `${before} → ${after}`);
  await wd("POST", `${S}/window`, { handle: browse });
  await shot("b3-imported.png");

  // STEP 6: a non-book download is refused and discarded.
  await typeAddress(SITE + "/notes.bin");
  const rejected = await until(async () => { const s = (await toolbar()).status; return /discarded/.test(s) ? s : null; }, 15000);
  check(!!rejected, "non-book downloads are discarded", rejected ?? (await toolbar()).status);

  // STEP 6b: third-party cookies are refused. 127.0.0.2 is a different site from
  // 127.0.0.1. Visiting it first (accepting the HTTP warning) also runs a
  // first-party cookie control, so the check below can actually fail.
  const OTHER = `http://127.0.0.2:${server.address().port}`;
  await typeAddress(OTHER + "/fp-set");
  if (await until(async () => (await toolbar()).warning)) {
    await exec(() => [...document.querySelectorAll(".warning button")].find((b) => b.innerText.includes("Continue")).click());
  }
  const fp = await until(() => reports.find((r) => "firstPartyCookie" in r), 15000);
  check(!!fp && fp.firstPartyCookie.includes("fp=1"), "control: first-party cookies work", fp ? JSON.stringify(fp.firstPartyCookie) : "no report");
  await typeAddress(SITE + "/tp-page");
  const tp = await until(() => reports.find((r) => "thirdPartyCookie" in r), 15000);
  check(!!tp && !/tp=1|tpjs=1/.test(tp.thirdPartyCookie), "third-party cookies are refused", tp ? JSON.stringify(tp.thirdPartyCookie) : "no report");

  // STEP 7: search uses DuckDuckGo by default, and a SearXNG template can be set.
  await typeAddress("moby dick epub");
  const ddg = await until(async () => { const a = (await toolbar()).address; return a.startsWith("https://duckduckgo.com/?q=moby+dick+epub") ? a : null; });
  check(!!ddg, "searches go to DuckDuckGo by default", ddg ?? (await toolbar()).address);
  await exec(() => document.querySelector('button[title="Search engine"]').click());
  await sleep(300);
  const setTemplate = (t) => exec((v) => {
    const i = document.querySelector(".template");
    i.value = v;
    i.dispatchEvent(new Event("input", { bubbles: true }));
    [...document.querySelectorAll(".settings button")].find((b) => b.innerText === "Save").click();
  }, t);
  await setTemplate("http://searx.example.org/search?q={q}");
  await sleep(500);
  await shot("b4-search-settings.png");
  check(/https:\/\//.test((await toolbar()).errorText), "insecure search templates are rejected", (await toolbar()).errorText);
  await setTemplate("https://searx.example.org/search?q={q}");
  await sleep(500);
  await typeAddress("treasure island");
  const sx = await until(async () => { const a = (await toolbar()).address; return a.startsWith("https://searx.example.org/search?q=treasure+island") ? a : null; });
  check(!!sx, "a self-hosted SearXNG template is used for searches", sx ?? (await toolbar()).address);
} catch (e) {
  failures++;
  console.log("FAIL  scenario aborted:", e.message);
  await shot("bz-failure.png").catch(() => {});
} finally {
  await wd("DELETE", S).catch(() => {});
  server.close();
}
console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
