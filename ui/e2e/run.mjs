// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// End-to-end check of the reader in a real layout engine (Chromium).
//
//   node e2e/run.mjs <library-data-dir> [screenshot-dir]
//
// The library dir is produced by `cargo run -p justread-library --example import`.

import { createServer } from "vite";
import { execFileSync } from "node:child_process";
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
const MIME = { ".json": "application/json", ".xhtml": "application/xhtml+xml", ".css": "text/css", ".png": "image/png", ".jpg": "image/jpeg", ".pdf": "application/pdf" };
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
execFileSync("node", ["scripts/copy-pdfjs.mjs"]); // PDF.js data must be in public/
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
/** Pick a layout from the toolbar's layout menu. */
const layout = async (label) => {
  await page.locator('button[title="Layout"]').click();
  await page.locator(".layout-menu button", { hasText: label }).click();
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
  check((await page.locator(".card").count()) === 8, "library lists all imported books (EPUB, TXT, PDF, CBZ, AZW3, MOBI, FB2, DOCX)");
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
  await layout("Scroll");
  await page.waitForSelector(".jr-scroller");
  await settle(1200);
  const L2 = await saved();
  check(L2 === L1, "pages → scroll keeps the exact locator", `${L1} vs ${L2}`);
  await page.screenshot({ path: join(shots, "03-scroll.png") });
  await layout("Single page");
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

  // STEP 8: layouts are exactly what the reader picks, whatever the window shape.
  await page.setViewportSize({ width: 1800, height: 900 });
  await page.locator('button[title="Layout"]').click();
  await page.screenshot({ path: join(shots, "05a-layout-menu.png"), clip: { x: 1300, y: 0, width: 500, height: 260 } });
  await page.keyboard.press("Escape");
  await layout("Two pages");
  await settle(1400);
  const colInfo = () => frameDoc((doc) => ({ col: parseFloat(getComputedStyle(doc.body).columnWidth), frame: doc.defaultView.innerWidth }));
  const cols = await colInfo();
  const p3 = await pageNo();
  check((await saved()) === L1, "switching to two pages keeps the locator", await saved());
  check(cols.col < cols.frame / 2, "Two pages shows two pages", `column ${cols.col}px in frame ${cols.frame}px; ${p2.pages} → ${p3?.pages} screens`);
  await page.setViewportSize({ width: 700, height: 1000 });
  await settle(1400);
  const narrow = await colInfo();
  check(narrow.col < narrow.frame / 2, "Two pages stays two pages in a narrow portrait window", JSON.stringify(narrow));
  await page.setViewportSize({ width: 1800, height: 900 });
  await layout("Single page");
  await settle(1400);
  const wideWin = await colInfo();
  check(wideWin.col > wideWin.frame * 0.9, "Single page stays one page in a very wide window", JSON.stringify(wideWin));
  check((await saved()) === L1, "…and every switch kept the locator", await saved());
  await layout("Two pages");
  await settle(1400);
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
  await layout("Scroll");
  await settle(1200);
  const marksScroll = await page.evaluate(() => [...document.querySelectorAll(".jr-scroller iframe")].reduce((n, f) => n + f.contentDocument.querySelectorAll("mark.jr-hl").length, 0));
  check(marksScroll > 0, "highlight is repainted in scroll mode");
  await layout("Two pages");
  await settle(1000);

  // STEP 10b: scrolling to the end of a chapter appends the next one.
  await layout("Scroll");
  await settle(1000);
  await page.evaluate(() => { const s = document.querySelector(".jr-scroller"); s.scrollTop = s.scrollHeight; });
  await settle(1200);
  const sections = await page.locator(".jr-scroller section").count();
  check(sections >= 2, "scrolling near the end loads the next chapter", `sections=${sections}`);
  check((await saved()) !== L1, "scrolling away moves the saved position", await saved());
  await layout("Two pages");
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

  // ===== Phase 3: fixed layouts =====================================
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.locator('button[title="Back to library (Esc)"]').click();
  await page.waitForSelector(".card");

  // STEP 16: the library renders a PDF cover with PDF.js and stores it.
  await page.waitForFunction(() => window.__jr.calls.some((c) => c.cmd === "set_cover"), null, { timeout: 15000 }).catch(() => {});
  const cover = await page.evaluate(() => window.__jr.calls.find((c) => c.cmd === "set_cover")?.args.bytes ?? []);
  check(cover.length > 1000 && cover[0] === 0xff && cover[1] === 0xd8, "PDF cover rendered to JPEG and stored", `${cover.length} bytes`);

  // STEP 17: open the PDF.
  await page.locator(".card", { hasText: "The Lantern Atlas" }).click();
  await page.waitForSelector(".jr-page canvas", { timeout: 15000 });
  await settle();
  check(/Page 1 of 8/.test(await posText()), "PDF opens on page 1 of 8", await posText());
  const ink = await page.evaluate(() => {
    const c = document.querySelector(".jr-page canvas");
    const d = c.getContext("2d").getImageData(0, 0, c.width, Math.floor(c.height / 3)).data;
    let dark = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] < 100 && d[i + 1] < 100 && d[i + 2] < 100) dark++;
    return dark;
  });
  check(ink > 200, "PDF text is actually drawn on the canvas", `${ink} dark pixels`);
  await page.screenshot({ path: join(shots, "08-pdf.png") });

  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await settle();
  const pdfLoc = await page.evaluate(() => [...window.__jr.progress.entries()].find(([, v]) => v.locator.startsWith("p:"))?.[1].locator);
  // "Two pages" (chosen earlier) applies to PDFs too: [1], [2–3], [4–5], …
  check(/Pages 4–5 of 8/.test(await posText()) && pdfLoc === "p:3@0", "PDF spread page turns save a page locator", `${await posText()} ${pdfLoc}`);

  // STEP 18: PDF in scroll mode keeps the page, renders lazily, and returns.
  await layout("Scroll");
  await settle(1200);
  const slots = await page.evaluate(() => ({ all: document.querySelectorAll(".jr-page").length, drawn: document.querySelectorAll(".jr-page canvas").length }));
  check(/Page 4 of 8/.test(await posText()), "PDF scroll mode opens at the same page", await posText());
  check(slots.all === 8 && slots.drawn >= 1 && slots.drawn < 8 + 1, "pages are placeholders until near the viewport", JSON.stringify(slots));
  await layout("Two pages");
  await settle();
  check(/Pages 4–5 of 8/.test(await posText()), "…and back to the same spread in paginated mode", await posText());

  // STEP 19: outline → TOC.
  await page.locator('button[title="Contents"]').click();
  const tocTitles = await page.locator(".side button").allInnerTexts();
  check(tocTitles.join("|") === "Part One|Part Two", "PDF outline becomes the table of contents", tocTitles.join("|"));
  await page.locator(".side button", { hasText: "Part Two" }).click();
  await settle();
  check(/Pages 4–5 of 8/.test(await posText()) && (await page.evaluate(() => [...window.__jr.progress.values()].find((v) => v.locator.startsWith("p:"))?.locator)) === "p:4@0", "outline entry jumps to its page (page 5)", await posText());

  // STEP 20: zoom with the keyboard; fit-width via settings.
  // Single pages for the zoom/fit checks.
  await layout("Single page");
  await settle();
  const w0 = await page.evaluate(() => document.querySelector(".jr-page").getBoundingClientRect().width);
  await page.keyboard.press("+");
  await settle(700);
  const w1 = await page.evaluate(() => document.querySelector(".jr-page").getBoundingClientRect().width);
  check(w1 > w0 * 1.05, "+ zooms in", `${Math.round(w0)} → ${Math.round(w1)}`);
  await page.keyboard.press("0");
  await settle(700);
  await page.locator('button[title="Reading settings"]').click();
  await page.locator("button", { hasText: "Fit width" }).click();
  await settle(800);
  const fw = await page.evaluate(() => ({ w: Math.round(document.querySelector(".jr-page").getBoundingClientRect().width), host: document.querySelector(".host").clientWidth }));
  check(fw.w > fw.host * 0.9, "fit width fills the reading area", JSON.stringify(fw));
  check(/Page 5 of 8/.test(await posText()), "fit change keeps the page", await posText());
  await page.screenshot({ path: join(shots, "10-pdf-fitwidth.png") });
  await page.locator("button", { hasText: "Fit page" }).click();
  await page.locator('button[title="Reading settings"]').click();
  await layout("Two pages");
  await settle();

  // STEP 21: bookmark a PDF page.
  await page.keyboard.press("b");
  await settle(500);
  const pdfBm = await page.evaluate(() => window.__jr.annotations.filter((a) => a.kind === "bookmark" && a.start.startsWith("p:")).map((a) => a.start));
  check(pdfBm.length === 1 && pdfBm[0] === "p:4@0", "bookmark on a PDF page stores a page locator", pdfBm.join(","));

  // STEP 22: the comic — natural page order, cover alone, landscape page alone.
  await page.locator('button[title="Back to library (Esc)"]').click();
  await page.locator(".card", { hasText: "Harbor Lights" }).click();
  await page.waitForSelector(".jr-page img");
  await settle();
  const shape = async () => page.evaluate(() => [...document.querySelectorAll(".jr-page")].map((s) => Number(s.dataset.page)));
  check(/Page 1 of 9/.test(await posText()), "comic opens on page 1 of 9", await posText());
  check((await shape()).length === 1, "cover is shown alone");
  await page.keyboard.press("ArrowRight");
  await settle(600);
  check(JSON.stringify(await shape()) === "[1,2]", "pages 2–3 form a spread", JSON.stringify(await shape()));
  await page.screenshot({ path: join(shots, "09-comic-spread.png") });
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await settle(600);
  const wide = await page.evaluate(() => { const i = document.querySelector(".jr-page img"); return { pages: document.querySelectorAll(".jr-page").length, w: i.naturalWidth, h: i.naturalHeight }; });
  check(wide.pages === 1 && wide.w > wide.h, "the landscape double page stands alone", JSON.stringify(wide));

  // ===== Phase 4: MOBI / AZW3 / FB2 / DOCX (generated from the sample EPUB) =====
  await page.setViewportSize({ width: 1000, height: 760 });
  for (const fmt of ["AZW3", "MOBI", "FB2", "DOCX"]) {
    await page.locator('button[title="Back to library (Esc)"]').click();
    await page.waitForSelector(".card");
    await page.locator(".card", { hasText: `Lantern (${fmt})` }).click();
    await page.waitForFunction(() => document.querySelector(".host iframe")?.contentDocument?.querySelector("[data-jrb]"), null, { timeout: 10000 });
    await settle();
    // Go to Chapter 1 through the TOC (some formats start with a cover or a TOC page).
    await page.locator('button[title="Contents"]').click();
    const tocTitles = await page.locator(".side button").allInnerTexts();
    await page.locator(".side button", { hasText: "Chapter 1" }).click();
    await settle();
    const info = await frameDoc((doc) => {
      const img = [...doc.querySelectorAll("img")].find((i) => i.naturalWidth === 240);
      return { text: doc.body.textContent.includes("Chapter 1"), img: img ? img.naturalWidth : -1 };
    });
    await page.locator('button[title="Contents"]').click();
    await page.locator(".side button", { hasText: "Chapter 3" }).click();
    await settle();
    const ch3 = await frameDoc((doc) => doc.body.textContent.includes("This is the paragraph the link in chapter one points to"));
    check(info.text && info.img > 0 && tocTitles.filter((t) => /^Chapter \d$/.test(t)).length === 6 && ch3,
      `${fmt}: renders text and images, 6-chapter TOC navigates`, JSON.stringify({ ...info, toc: tocTitles.length, ch3 }));
    if (fmt === "AZW3") await page.screenshot({ path: join(shots, "11-azw3.png") });
  }

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
