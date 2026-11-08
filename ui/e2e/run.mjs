// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// End-to-end check of the reader in a real layout engine (Chromium).
//
//   node e2e/run.mjs <library-data-dir> [screenshot-dir]
//
// The library dir is produced by `cargo run -p justread-library --example import`.

import { createServer } from "vite";
import { createRequire } from "node:module";
import { readdirSync, readFileSync, existsSync, mkdirSync } from "node:fs";
import { join, normalize, resolve, extname } from "node:path";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");

const dataDir = resolve(process.argv[2] ?? "");
const shots = resolve(process.argv[3] ?? "e2e-shots");
mkdirSync(shots, { recursive: true });
const booksDir = join(dataDir, "books");
if (!existsSync(booksDir)) throw new Error(`no books in ${booksDir}`);

// STEP 1: dev server + a tiny static handler for the packages.
const MIME = { ".json": "application/json", ".xhtml": "application/xhtml+xml", ".css": "text/css", ".png": "image/png", ".jpg": "image/jpeg" };
const booksPlugin = {
  name: "justread-e2e-books",
  configureServer(server) {
    server.middlewares.use("/__books", (req, res) => {
      const path = decodeURIComponent((req.url ?? "/").split("?")[0]);
      if (path === "/ids.json") {
        res.setHeader("Content-Type", "application/json");
        return res.end(JSON.stringify(readdirSync(booksDir).filter((d) => !d.endsWith(".partial"))));
      }
      const file = normalize(join(booksDir, path));
      if (!file.startsWith(booksDir) || !existsSync(file)) {
        res.statusCode = 404;
        return res.end();
      }
      res.setHeader("Content-Type", MIME[extname(file)] ?? "application/octet-stream");
      res.end(readFileSync(file));
    });
  },
};
const server = await createServer({ root: resolve("."), server: { port: 5174, strictPort: true }, plugins: [booksPlugin], logLevel: "warn" });
await server.listen();

// STEP 2: helpers.
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1000, height: 760 }, deviceScaleFactor: 1 });
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));

let failures = 0;
function check(cond, label, extra = "") {
  console.log(`${cond ? "PASS" : "FAIL"}  ${label}${extra ? `  (${extra})` : ""}`);
  if (!cond) failures++;
}
const settle = (ms = 900) => page.waitForTimeout(ms);
const saved = () => page.evaluate(() => [...window.__jr.progress.values()][0]?.locator ?? null);
const posText = () => page.locator(".pos").innerText();
const pageNo = async () => {
  const m = /Page (\d+) of (\d+)/.exec(await posText());
  return m ? { page: +m[1], pages: +m[2] } : null;
};
const frameDoc = (fn, arg) =>
  page.evaluate(([src, a]) => {
    const f = document.querySelector(".host iframe");
    return new Function("doc", "arg", `return (${src})(doc, arg)`)(f.contentDocument, a);
  }, [fn.toString(), arg]);

try {
  // STEP 3: library.
  await page.goto("http://localhost:5174/e2e/index.html");
  await page.waitForSelector(".card");
  check((await page.locator(".card").count()) === 2, "library lists both imported books");
  await page.screenshot({ path: join(shots, "01-library.png") });

  // STEP 4: open the EPUB; paginated by default.
  await page.locator(".card", { hasText: "The Lantern Harbor" }).click();
  await page.waitForFunction(() => document.querySelector(".host iframe")?.contentDocument?.querySelector("[data-jrb]"));
  await settle();
  const p0 = await pageNo();
  check(p0 && p0.page === 1 && p0.pages > 2, "opens on page 1 of a multi-page chapter", JSON.stringify(p0));
  const img = await frameDoc((doc) => { const i = doc.querySelector("img"); return i ? i.naturalWidth : -1; });
  check(img > 0, "book image loads through the package base URL", `naturalWidth=${img}`);
  await page.screenshot({ path: join(shots, "02-reader-page1.png") });

  // STEP 5: turn pages with the keyboard.
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
  await settle();
  const p1 = await pageNo();
  const L1 = await saved();
  check(p1?.page === 4, "ArrowRight ×3 → page 4", JSON.stringify(p1));
  check(!!L1 && L1.startsWith("f:0/"), "progress saved as a flow locator", L1);

  // STEP 6: switch to scroll mode and back — position must be identical.
  await page.locator('button[title="Switch to scrolling"]').click();
  await page.waitForSelector(".jr-scroller");
  await settle(1200);
  const L2 = await saved();
  check(L2 === L1, "pages → scroll keeps the exact locator", `${L1} vs ${L2}`);
  await page.screenshot({ path: join(shots, "03-scroll.png") });
  await page.locator('button[title="Switch to pages"]').click();
  await settle(1200);
  check((await saved()) === L1, "scroll → pages returns to the same locator", await saved());
  check((await pageNo())?.page === 4, "…and the same page number", JSON.stringify(await pageNo()));

  // STEP 7: change font size via the settings panel — locator stays, page count changes.
  await page.locator('button[title="Reading settings"]').click();
  const slider = page.locator('input[aria-label="Font size"]');
  await slider.evaluate((el) => { el.value = "26"; el.dispatchEvent(new Event("input", { bubbles: true })); el.dispatchEvent(new Event("change", { bubbles: true })); });
  await settle(1200);
  const p2 = await pageNo();
  check((await saved()) === L1, "font size change keeps the locator", await saved());
  check(p2 && p2.pages > p0.pages, "bigger font → more pages", `${p0.pages} → ${p2?.pages}`);
  const visible = await frameDoc((doc, loc) => {
    // Is the anchor's block on the visible screen?
    const b = doc.querySelector(`[data-jrb="${loc.split("/")[1].split(":")[0]}"]`);
    const rects = [...b.getClientRects()];
    return rects.some((r) => r.right > 0 && r.left < doc.defaultView.innerWidth);
  }, L1);
  check(visible, "anchor block is on screen after re-layout");
  await page.locator('button[title="Reading settings"]').click();
  await page.screenshot({ path: join(shots, "04-font26.png") });

  // STEP 8: wide window → automatic two-page spread.
  // At 26px a spread needs ~24em per column, so make the window wide enough.
  await page.setViewportSize({ width: 1800, height: 900 });
  await settle(1400);
  const cols = await frameDoc((doc) => ({ col: parseFloat(getComputedStyle(doc.body).columnWidth), frame: doc.defaultView.innerWidth }));
  const p3 = await pageNo();
  check((await saved()) === L1, "resize to spread keeps the locator", await saved());
  check(cols.col < cols.frame / 2, "wide window shows a two-page spread", `column ${cols.col}px in frame ${cols.frame}px; ${p2.pages} → ${p3?.pages} screens`);
  await page.screenshot({ path: join(shots, "05-spread.png") });

  // STEP 9: select text → highlight (also proves parent listeners work in the sandboxed iframe).
  await frameDoc((doc) => {
    const p = [...doc.querySelectorAll("p[data-jrb]")].find((el) => {
      const r = el.getBoundingClientRect();
      return r.left >= 0 && r.right <= doc.defaultView.innerWidth && el.textContent.length > 40;
    });
    const t = p.firstChild;
    const r = doc.createRange();
    r.setStart(t, 0);
    r.setEnd(t, 30);
    const sel = doc.getSelection();
    sel.removeAllRanges();
    sel.addRange(r);
    doc.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  });
  await page.waitForSelector(".menu", { timeout: 3000 });
  check(true, "selection menu appears for a text selection");
  await page.locator(".menu .swatch").first().click();
  await settle(500);
  const anns = await page.evaluate(() => window.__jr.annotations);
  check(anns.length === 1 && anns[0].kind === "highlight" && anns[0].quote.length > 0, "highlight saved with quote and locators", anns[0] && `${anns[0].start}..${anns[0].end}`);
  const marks = await frameDoc((doc) => doc.querySelectorAll("mark.jr-hl").length);
  check(marks > 0, "highlight painted in the chapter", `marks=${marks}`);
  await page.screenshot({ path: join(shots, "06-highlight.png") });

  // STEP 10: highlight survives a layout switch (painted again in the new view).
  await page.locator('button[title="Switch to scrolling"]').click();
  await settle(1200);
  const marksScroll = await page.evaluate(() => [...document.querySelectorAll(".jr-scroller iframe")].reduce((n, f) => n + f.contentDocument.querySelectorAll("mark.jr-hl").length, 0));
  check(marksScroll > 0, "highlight is repainted in scroll mode");
  await page.locator('button[title="Switch to pages"]').click();
  await settle(1000);

  // STEP 10b: scrolling to the end of a chapter appends the next one.
  await page.locator('button[title="Switch to scrolling"]').click();
  await settle(1000);
  await page.evaluate(() => { const s = document.querySelector(".jr-scroller"); s.scrollTop = s.scrollHeight; });
  await settle(1200);
  const sections = await page.locator(".jr-scroller section").count();
  check(sections >= 2, "scrolling near the end loads the next chapter", `sections=${sections}`);
  check((await saved()) !== L1, "scrolling away moves the saved position", await saved());
  await page.locator('button[title="Switch to pages"]').click();
  await settle(1000);

  // STEP 11: bookmark with the keyboard.
  await page.keyboard.press("b");
  await settle(500);
  const bms = await page.evaluate(() => window.__jr.annotations.filter((a) => a.kind === "bookmark"));
  check(bms.length === 1, "B adds a bookmark at the current page", bms[0]?.start);
  check(await page.locator('button[title="Bookmark this page (B)"].active').count() === 1, "bookmark button shows the page is bookmarked");

  // STEP 12: TOC navigation and an internal link.
  await page.locator('button[title="Contents"]').click();
  await page.locator(".side button", { hasText: "Chapter 3" }).click();
  await settle();
  check((await page.locator(".chapter").innerText()) === "Chapter 3", "TOC jumps to Chapter 3");
  await page.locator('button[title="Contents"]').click();
  await page.locator(".side button", { hasText: "Chapter 1" }).click();
  await settle();
  await frameDoc((doc) => doc.querySelector('a[data-jr-href*="#note"]').click());
  await settle(1000);
  const noteVisible = await frameDoc((doc) => {
    const r = doc.getElementById("note")?.getBoundingClientRect();
    return !!r && r.right > 0 && r.left < doc.defaultView.innerWidth;
  });
  check((await page.locator(".chapter").innerText()) === "Chapter 3" && noteVisible, "internal link opens chapter 3 at #note");

  // STEP 13: dark theme applies to chrome and page.
  await page.locator('button[title="Reading settings"]').click();
  await page.locator('button.theme[title="Dark"]').click();
  await settle(800);
  const bg = await frameDoc((doc) => getComputedStyle(doc.body).backgroundColor);
  check(bg === "rgb(30, 31, 34)", "dark theme reaches the chapter", bg);
  await page.locator('button[title="Reading settings"]').click();
  await page.screenshot({ path: join(shots, "07-dark.png") });

  // STEP 14: reopen the book — it resumes at the saved locator.
  const before = await saved();
  await page.locator('button[title="Back to library (Esc)"]').click();
  await page.waitForSelector(".card");
  await page.locator(".card", { hasText: "The Lantern Harbor" }).click();
  await settle(1500);
  check((await saved()) === before, "reopening resumes at the saved position", `${before} vs ${await saved()}`);
  check((await page.locator(".chapter").innerText()) === "Chapter 3", "…in the right chapter");

  // STEP 15: the TXT book opens too.
  await page.locator('button[title="Back to library (Esc)"]').click();
  await page.locator(".card", { hasText: "A Plain Text Tale" }).click();
  await settle();
  const txt = await frameDoc((doc) => doc.body.textContent);
  check(txt.includes("quiet morning"), "TXT book renders");

  check(errors.length === 0, "no page errors", errors.slice(0, 3).join(" | "));
} catch (e) {
  failures++;
  console.log("FAIL  scenario aborted:", e.message);
  await page.screenshot({ path: join(shots, "zz-failure.png") }).catch(() => {});
} finally {
  await browser.close();
  await server.close();
}
console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
