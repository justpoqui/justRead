// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// TypeScript twin of crates/justread-locator. Both implementations are
// tested against tests/fixtures/locators.json so they can never drift.
// Grammar (docs/LOCATOR_SPEC.md):
//   flow   f:<spine>/<block>:<offset>   e.g. f:3/127:42
//   fixed  p:<page>@<fraction>          e.g. p:12@0.25

export interface FlowLocator {
  kind: "flow";
  spine: number;
  block: number;
  offset: number;
}

export interface FixedLocator {
  kind: "fixed";
  page: number;
  offset: number;
}

export type Locator = FlowLocator | FixedLocator;

const U32_MAX = 4294967295;

/** Strict canonical unsigned integer (no sign, no leading zeros). */
function parseU32(s: string): number | null {
  if (!/^(0|[1-9][0-9]*)$/.test(s)) return null;
  const n = Number(s);
  return n <= U32_MAX ? n : null;
}

/** Parse the text form; `null` if malformed. */
export function parseLocator(text: string): Locator | null {
  // STEP 1: kind prefix.
  const colon = text.indexOf(":");
  if (colon < 0) return null;
  const kind = text.slice(0, colon);
  const rest = text.slice(colon + 1);

  // STEP 2a: flow.
  if (kind === "f") {
    const m = /^([^/]*)\/([^:]*):(.*)$/.exec(rest);
    if (!m) return null;
    const spine = parseU32(m[1]!);
    const block = parseU32(m[2]!);
    const offset = parseU32(m[3]!);
    if (spine === null || block === null || offset === null) return null;
    return { kind: "flow", spine, block, offset };
  }

  // STEP 2b: fixed.
  if (kind === "p") {
    const at = rest.indexOf("@");
    if (at < 0) return null;
    const page = parseU32(rest.slice(0, at));
    const frac = rest.slice(at + 1);
    if (page === null || !/^[0-9.eE-]+$/.test(frac)) return null;
    const offset = Number(frac);
    if (!Number.isFinite(offset) || offset < 0 || offset > 1) return null;
    return { kind: "fixed", page, offset };
  }
  return null;
}

/** Format to the canonical text form. */
export function formatLocator(l: Locator): string {
  if (l.kind === "flow") return `f:${l.spine}/${l.block}:${l.offset}`;
  // Four decimals, trailing zeros trimmed — identical to the Rust formatter.
  const fixed = Math.min(1, Math.max(0, l.offset)).toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
  return `p:${l.page}@${fixed === "" ? "0" : fixed}`;
}

/** Reading-order comparison; `null` when kinds differ. */
export function compareLocators(a: Locator, b: Locator): number | null {
  if (a.kind === "flow" && b.kind === "flow") {
    return a.spine - b.spine || a.block - b.block || a.offset - b.offset;
  }
  if (a.kind === "fixed" && b.kind === "fixed") {
    return a.page - b.page || a.offset - b.offset;
  }
  return null;
}

export interface SpineStats {
  chars: number;
  blocks: number;
}

/** Whole-book progression estimate; mirrors `justread_locator::progression`. */
export function progression(spine: SpineStats[], loc: FlowLocator): number {
  const total = spine.reduce((s, x) => s + x.chars, 0);
  if (total === 0 || spine.length === 0) return 0;
  const idx = Math.min(loc.spine, spine.length - 1);
  const before = spine.slice(0, idx).reduce((s, x) => s + x.chars, 0);
  const here = spine[idx]!;
  const frac = here.blocks <= 1 ? 0 : loc.block / here.blocks;
  const within = here.chars * Math.min(1, Math.max(0, frac));
  return Math.min(1, Math.max(0, (before + within) / total));
}

/** Inverse of `progression`; mirrors `justread_locator::locate_progression`. */
export function locateProgression(spine: SpineStats[], fraction: number): FlowLocator {
  const total = spine.reduce((s, x) => s + x.chars, 0);
  const target = Math.floor(Math.min(1, Math.max(0, fraction)) * total);
  let acc = 0;
  for (let i = 0; i < spine.length; i++) {
    const s = spine[i]!;
    if (target < acc + s.chars || i === spine.length - 1) {
      const into = (target - acc) / Math.max(1, s.chars);
      const block = Math.min(Math.floor(into * s.blocks), Math.max(0, s.blocks - 1));
      return { kind: "flow", spine: i, block: Math.max(0, block), offset: 0 };
    }
    acc += s.chars;
  }
  return { kind: "flow", spine: 0, block: 0, offset: 0 };
}
