// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Copy the PDF.js runtime data into public/pdfjs/ so it is bundled with the
// app and served locally (offline-only: nothing is fetched from a CDN).
//   cmaps/           character maps for CJK and other non-Latin PDFs
//   standard_fonts/  metrics for the 14 standard PDF fonts
//   wasm/            JBIG2, OpenJPEG and QCMS image/color decoders
// The QuickJS engine (PDF form scripting) is deliberately NOT copied:
// justRead never runs scripts embedded in documents.

import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = dirname(require.resolve("pdfjs-dist/package.json"));
const dest = join(dirname(fileURLToPath(import.meta.url)), "..", "public", "pdfjs");

// STEP 1: start clean so removed files don't linger.
if (existsSync(dest)) rmSync(dest, { recursive: true });
mkdirSync(dest, { recursive: true });

// STEP 2: data folders.
for (const dir of ["cmaps", "standard_fonts"]) cpSync(join(root, dir), join(dest, dir), { recursive: true });

// STEP 3: wasm decoders and their licenses, minus QuickJS.
mkdirSync(join(dest, "wasm"));
for (const f of readdirSync(join(root, "wasm"))) {
  if (!/quickjs/i.test(f)) cpSync(join(root, "wasm", f), join(dest, "wasm", f));
}
cpSync(join(root, "LICENSE"), join(dest, "LICENSE"));
console.log(`pdf.js assets → ${dest}`);
