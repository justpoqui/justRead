// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Phone-sized end-to-end check (Chromium, Pixel 7 emulation) with real touch
// input sent through the DevTools protocol, so swipes and pinches are
// hit-tested into the chapter iframes exactly like a finger would be.
//
//   node e2e/mobile.mjs <library-data-dir> [screenshot-dir]

import { createServer } from "vite";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, existsSync, mkdirSync } from "node:fs";
import { join, normalize, resolve, extname } from "node:path";

const require = createRequire(import.meta.url);
const { chromium, devices } = require("playwright");

const booksDir = join(resolve(process.argv[2] ?? ""), "books");
const shots = resolve(process.argv[3] ?? "e2e-shots-mobile");
mkdirSync(shots, { recursive: true });

// STEP 1: the same dev server + package handler as e2e/run.mjs.
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
execFileSync("node", ["scripts/copy-pdfjs.mjs"]);
const server = await createServer({ root: resolve("."), server: { port: 5175, strictPort: true }, plugins: [booksPlugin], logLevel: "warn" });
await server.listen();

const browser = await chromium.launch();
const context = await browser.newContext({ ...devices["Pixel 7"] });
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));

let failures = 0;
const check = (cond, label, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${label}${extra ? `  (${extra})` : ""}`);
  if (!cond) failures++;
};
const settle = (ms = 900) => page.waitForTimeout(ms);
const pos = () => page.locator(".pos-full").textContent();

// STEP 2: touch helpers (one finger swipe, two finger pinch).
async function touch(type, pts) {
  await cdp.send("Input.dispatchTouchEvent", { type, touchPoints: pts.map(([x, y], id) => ({ x, y, id })) });
}
async function swipe(x1, y, x2) {
  await touch("touchStart", [[x1, y]]);
  for (let i = 1; i <= 6; i++) {
    await touch("touchMove", [[x1 + ((x2 - x1) * i) / 6, y]]);
    await page.waitForTimeout(16);
  }
  await touch("touchEnd", []);
}
async function pinch(cx, cy, from, to) {
  await touch("touchStart", [[cx - from, cy], [cx + from, cy]]);
  for (let i = 1; i <= 8; i++) {
    const d = from + ((to - from) * i) / 8;
    await touch("touchMove", [[cx - d, cy], [cx + d, cy]]);
    await page.waitForTimeout(16);
  }
  await touch("touchEnd", []);
}
const vp = page.viewportSize();

try {
  // STEP 3: library at phone width.
  await page.goto("http://localhost:5175/e2e/index.html");
  await page.waitForSelector(".card");
  const lay = await page.evaluate(() => ({ overflow: document.documentElement.scrollWidth - window.innerWidth, width: window.innerWidth }));
  check(lay.width === vp.width, "phone viewport is in effect (no 980px desktop fallback)", `${lay.width}px`);
  check(lay.overflow <= 0, "library fits the phone width (no sideways scrolling)", `overflow ${lay.overflow}px`);
  check((await page.locator("text=Find books").count()) === 0, "justBrowse button hidden on phones (desktop-only for now)");
  await page.screenshot({ path: join(shots, "m1-library.png") });

  // STEP 4: swipe through an EPUB.
  await page.locator(".card", { hasText: "The Lantern Harbor" }).click();
  await page.waitForFunction(() => document.querySelector(".host iframe")?.contentDocument?.querySelector("[data-jrb]"));
  await settle();
  check(/Page 1 of/.test(await pos()), "EPUB opens on page 1", await pos());
  check(await page.locator(".edge").count() === 0 || !(await page.locator(".edge").first().isVisible()), "edge arrows hidden on touch screens");
  await swipe(vp.width * 0.8, vp.height * 0.5, vp.width * 0.2);
  await settle();
  check(/Page 2 of/.test(await pos()), "swipe left turns to the next page", await pos());
  await swipe(vp.width * 0.2, vp.height * 0.5, vp.width * 0.8);
  await settle();
  check(/Page 1 of/.test(await pos()), "swipe right turns back", await pos());
  await page.screenshot({ path: join(shots, "m2-reader.png") });

  // STEP 5: a long-press selection reports through selectionchange (no mouseup on phones).
  await page.evaluate(() => {
    const doc = document.querySelector(".host iframe").contentDocument;
    // A long, visible text node on the current page.
    const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT);
    let t;
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const rect = n.parentElement.getBoundingClientRect();
      if (n.data.length > 40 && rect.left >= 0 && rect.left < doc.defaultView.innerWidth) { t = n; break; }
    }
    const r = doc.createRange();
    r.setStart(t, 0);
    r.setEnd(t, 20);
    doc.getSelection().removeAllRanges();
    doc.getSelection().addRange(r);
  });
  await page.waitForSelector(".menu", { timeout: 3000 }).catch(() => {});
  check((await page.locator(".menu").count()) === 1, "touch selection shows the highlight menu");
  // The phone's own Copy/Share bar sits above the selection: ours goes below and has no Copy.
  const selBottom = await page.evaluate(() => {
    const f = document.querySelector(".host iframe");
    const r = f.contentDocument.getSelection().getRangeAt(0).getBoundingClientRect();
    return f.getBoundingClientRect().top + r.bottom;
  });
  const menuBox = await page.locator(".menu").boundingBox();
  check(menuBox.y > selBottom, "on touch, our menu sits below the selection (clear of the system bar)", `menu ${Math.round(menuBox.y)} > selection ${Math.round(selBottom)}`);
  check((await page.locator('.menu button[title="Copy"]').count()) === 0, "no duplicate Copy button on touch (the system bar has it)");
  await page.screenshot({ path: join(shots, "m3-selection.png") });
  await page.evaluate(() => document.querySelector(".host iframe").contentDocument.getSelection().removeAllRanges());
  await settle(600);

  // STEP 5b: tapping the middle hides the bars; hidden controls ignore taps; page / % stays.
  await page.touchscreen.tap(vp.width / 2, vp.height / 2);
  await settle(500);
  const hidden = await page.evaluate(() => {
    const btn = document.querySelector('button[title="Contents"]');
    const r = btn.getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const pos = document.querySelector(".pos");
    const full = document.querySelector(".pos-full");
    return {
      topVisibility: getComputedStyle(document.querySelector(".top")).visibility,
      hitsButton: !!hit && (hit === btn || btn.contains(hit)),
      posOpacity: Number(getComputedStyle(pos).opacity),
      posText: full.textContent,
    };
  });
  check(hidden.topVisibility === "hidden" && !hidden.hitsButton, "hidden toolbar buttons can't be tapped", JSON.stringify(hidden));
  check(hidden.posOpacity > 0.3 && /Page \d+ of/.test(hidden.posText), "page / % stays visible while the bars are hidden", JSON.stringify(hidden));
  await page.screenshot({ path: join(shots, "m3b-hidden-chrome.png") });
  await page.touchscreen.tap(vp.width / 2, vp.height / 2);
  await settle(500);
  check(await page.evaluate(() => getComputedStyle(document.querySelector(".top")).visibility === "visible"), "tapping the middle again brings the bars back");

  // STEP 6: panels are bottom sheets.
  await page.locator('button[title="Reading settings"]').click();
  const sheet = await page.locator(".side").boundingBox();
  check(sheet && Math.abs(sheet.x) < 1 && Math.abs(sheet.width - vp.width) < 2 && Math.abs(sheet.y + sheet.height - (vp.height - (await page.locator(".bottom").boundingBox()).height)) < 4,
    "settings open as a full-width bottom sheet", JSON.stringify(sheet));
  await page.screenshot({ path: join(shots, "m4-settings-sheet.png") });
  await page.locator('button[title="Reading settings"]').click();

  // STEP 7: comic — swipe and pinch-zoom.
  await page.locator('button[title="Back to library (Esc)"]').click();
  await page.locator(".card", { hasText: "Harbor Lights" }).click();
  await page.waitForSelector(".jr-page img");
  await settle();
  await swipe(vp.width * 0.8, vp.height * 0.5, vp.width * 0.2);
  await settle();
  check(/Page 2 of 9/.test(await pos()), "swipe turns comic pages", await pos());
  const w0 = (await page.locator(".jr-page").first().boundingBox()).width;
  await pinch(vp.width / 2, vp.height / 2, 60, 150);
  await settle(800);
  const w1 = (await page.locator(".jr-page").first().boundingBox()).width;
  check(w1 > w0 * 1.5, "pinch out zooms the page", `${Math.round(w0)} → ${Math.round(w1)}px`);
  // Zoomed in, a one-finger swipe pans instead of turning the page.
  await swipe(vp.width * 0.8, vp.height * 0.5, vp.width * 0.2);
  await settle();
  check(/Page 2 of 9/.test(await pos()), "when zoomed, swiping pans instead of turning", await pos());
  await page.screenshot({ path: join(shots, "m5-comic-zoom.png") });
  await pinch(vp.width / 2, vp.height / 2, 150, 40);
  await settle(800);
  const w2 = (await page.locator(".jr-page").first().boundingBox()).width;
  check(w2 < w1, "pinch in zooms back out", `${Math.round(w1)} → ${Math.round(w2)}px`);

  check(errors.length === 0, "no page errors", errors.slice(0, 3).join(" | "));
} catch (e) {
  failures++;
  console.log("FAIL  scenario aborted:", e.message);
  await page.screenshot({ path: join(shots, "mz-failure.png") }).catch(() => {});
} finally {
  await browser.close();
  await server.close();
}
console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
