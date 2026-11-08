// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Reader settings model: typography, theme and layout. Stored as JSON in
// the SQLite `settings` table under the key "reader".

export type FontChoice = "publisher" | "serif" | "sans" | "humanist" | "mono" | "custom";
export type ThemeId = "light" | "sepia" | "dark" | "black" | "custom";
export type FlowMode = "paginated" | "scrolled";
/** Pages per screen in paginated mode — always exactly what the reader chose. */
export type SpreadMode = "single" | "double";

/** Buttons the reader toolbar can show (Settings → Toolbar). */
export type ToolbarItem = "toc" | "notes" | "bookmark" | "layout" | "text" | "position";
export const TOOLBAR_ITEMS: { id: ToolbarItem; label: string }[] = [
  { id: "toc", label: "Contents" },
  { id: "notes", label: "Highlights & bookmarks" },
  { id: "bookmark", label: "Bookmark this page" },
  { id: "layout", label: "Layout" },
  { id: "text", label: "Quick text settings (Aa)" },
  { id: "position", label: "Go to position" },
];
/** Default: everything that was in the toolbar before, plus Position. */
export const DEFAULT_TOOLBAR: ToolbarItem[] = ["toc", "notes", "bookmark", "layout", "text", "position"];

/** What the page indicator shows; tapping it cycles through the available ones. */
export type ProgressDisplay = "chapterPages" | "bookPages" | "chapterPercent" | "bookPercent";
export const PROGRESS_DISPLAYS: { id: ProgressDisplay; label: string }[] = [
  { id: "chapterPages", label: "Pages left in chapter" },
  { id: "bookPages", label: "Pages left in book" },
  { id: "chapterPercent", label: "% of chapter" },
  { id: "bookPercent", label: "% of book" },
];

export interface ReaderSettings {
  font: FontChoice;
  /** Family name used when `font` is "custom" (any installed font). */
  customFont: string;
  /** Base font size in CSS px. */
  fontSize: number;
  lineHeight: number;
  /** Page margin in CSS px (horizontal; vertical is derived). */
  margin: number;
  /** Maximum line length in characters for readability on wide screens (scroll mode). */
  maxLineChars: number;
  hyphenation: boolean;
  justify: boolean;
  /** Keep the book's own fonts/spacing where we don't override. */
  publisherStyles: boolean;
  theme: ThemeId;
  customColors: { background: string; text: string; link: string };
  flow: FlowMode;
  spread: SpreadMode;
  /** Fixed-layout books: fit the whole page or the page width. */
  fit: "page" | "width";
  /** Fixed-layout zoom on top of the fit (1 = 100 %). */
  zoom: number;
  /** Toolbar buttons, in order. */
  toolbar: ToolbarItem[];
  /** Where the toolbar buttons sit (back, title and ⚙ always stay on top). */
  toolbarPosition: "top" | "bottom";
  progressDisplay: ProgressDisplay;
}

export const DEFAULT_SETTINGS: ReaderSettings = {
  font: "serif",
  customFont: "",
  fontSize: 19,
  lineHeight: 1.55,
  margin: 48,
  maxLineChars: 72,
  hyphenation: true,
  justify: true,
  publisherStyles: true,
  theme: "light",
  customColors: { background: "#1d2b2f", text: "#d9e4dd", link: "#8fc7b5" },
  flow: "paginated",
  spread: "single",
  fit: "page",
  zoom: 1,
  toolbar: [...DEFAULT_TOOLBAR],
  toolbarPosition: "bottom",
  progressDisplay: "chapterPages",
};

/** System font stacks — nothing is downloaded or bundled (offline, no OFL dependency). */
export const FONT_STACKS: Record<Exclude<FontChoice, "publisher" | "custom">, string> = {
  serif: `"Iowan Old Style", "Palatino Linotype", Palatino, "Book Antiqua", Georgia, "Noto Serif", "DejaVu Serif", serif`,
  sans: `system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", Ubuntu, Cantarell, "DejaVu Sans", sans-serif`,
  humanist: `Seravek, "Gill Sans Nova", Ubuntu, Calibri, "DejaVu Sans", source-sans-pro, sans-serif`,
  mono: `ui-monospace, "Cascadia Code", "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace`,
};

export interface ThemeColors {
  background: string;
  text: string;
  link: string;
  /** Whether book colors must be overridden (dark-ish themes). */
  forceColors: boolean;
}

export const THEMES: Record<Exclude<ThemeId, "custom">, ThemeColors> = {
  light: { background: "#fdfcf9", text: "#1f1f1f", link: "#2a5db0", forceColors: false },
  sepia: { background: "#f4ecd8", text: "#3b2f20", link: "#7a4b12", forceColors: false },
  dark: { background: "#1e1f22", text: "#d8d6d0", link: "#8ab4f8", forceColors: true },
  black: { background: "#000000", text: "#c8c8c8", link: "#8ab4f8", forceColors: true },
};

export function themeColors(s: ReaderSettings): ThemeColors {
  if (s.theme === "custom") return { ...s.customColors, forceColors: true };
  return THEMES[s.theme];
}

/** Clamp/repair a stored settings object (it may come from an older version). */
export function sanitizeSettings(raw: unknown): ReaderSettings {
  const s = { ...DEFAULT_SETTINGS, ...(typeof raw === "object" && raw ? (raw as Partial<ReaderSettings>) : {}) };
  const clamp = (v: unknown, lo: number, hi: number, d: number) =>
    typeof v === "number" && Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : d;
  s.fontSize = clamp(s.fontSize, 10, 48, DEFAULT_SETTINGS.fontSize);
  s.lineHeight = clamp(s.lineHeight, 1, 2.6, DEFAULT_SETTINGS.lineHeight);
  s.margin = clamp(s.margin, 0, 160, DEFAULT_SETTINGS.margin);
  s.maxLineChars = clamp(s.maxLineChars, 30, 200, DEFAULT_SETTINGS.maxLineChars);
  if (!["publisher", "serif", "sans", "humanist", "mono", "custom"].includes(s.font)) s.font = DEFAULT_SETTINGS.font;
  if (!["light", "sepia", "dark", "black", "custom"].includes(s.theme)) s.theme = "light";
  if (!["paginated", "scrolled"].includes(s.flow)) s.flow = "paginated";
  // Older versions stored "auto"/"always"/"never"; only an explicit "always" meant two pages.
  s.spread = (s.spread as string) === "double" || (s.spread as string) === "always" ? "double" : "single";
  if (!["page", "width"].includes(s.fit)) s.fit = "page";
  s.zoom = clamp(s.zoom, 0.5, 5, 1);
  // Toolbar: known items only, no duplicates (an empty toolbar is allowed).
  const known = new Set(TOOLBAR_ITEMS.map((t) => t.id));
  s.toolbar = Array.isArray(s.toolbar)
    ? s.toolbar.filter((t, i, a): t is ToolbarItem => known.has(t) && a.indexOf(t) === i)
    : [...DEFAULT_TOOLBAR];
  if (!["top", "bottom"].includes(s.toolbarPosition)) s.toolbarPosition = "bottom";
  if (!PROGRESS_DISPLAYS.some((p) => p.id === s.progressDisplay)) s.progressDisplay = "chapterPages";
  // A custom font name is inserted into CSS: allow only safe characters.
  s.customFont = String(s.customFont ?? "").replace(/[^\p{L}\p{N} _-]/gu, "").slice(0, 64);
  s.customColors = { ...DEFAULT_SETTINGS.customColors, ...(s.customColors ?? {}) };
  for (const k of ["background", "text", "link"] as const) {
    if (!/^#[0-9a-fA-F]{3,8}$/.test(s.customColors[k])) s.customColors[k] = DEFAULT_SETTINGS.customColors[k];
  }
  return s;
}
