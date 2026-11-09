// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Types mirroring the Rust side (crates/justbook/src/manifest.rs and
// crates/justread-library/src/models.rs). Keep them in sync.

export interface Metadata {
  title: string;
  authors: string[];
  language: string | null;
  publisher: string | null;
  description: string | null;
  published: string | null;
  series: string | null;
  seriesIndex: number | null;
  identifiers: string[];
}

export interface SpineItem {
  href: string;
  title: string | null;
  chars: number;
  blocks: number;
  linear: boolean;
}

export interface TocEntry {
  title: string;
  href: string | null;
  children?: TocEntry[];
}

export interface Manifest {
  schemaVersion: number;
  id: string;
  layout: "reflowable" | "fixed";
  metadata: Metadata;
  spine: SpineItem[];
  toc: TocEntry[];
  resources: { href: string; mediaType: string }[];
  cover: string | null;
  source: { format: string; fileName: string; sha256: string; converterVersion: string };
  fixed?: FixedContent | null;
}

export interface FixedContent {
  kind: "pdf" | "images";
  pageCount: number;
  document?: string | null;
  pages: { href: string; width: number; height: number }[];
  rightToLeft: boolean;
}

export interface BookSummary {
  id: string;
  title: string;
  authors: string[];
  series: string | null;
  seriesIndex: number | null;
  language: string | null;
  format: string;
  cover: string | null;
  addedAt: number;
  lastOpenedAt: number | null;
  progression: number;
  status: BookStatus;
  finishedAt: number | null;
  /** Ids of the collections the book is in. */
  collections: string[];
}

/** Library shelves (Phase 8). */
export type BookStatus = "unread" | "reading" | "finished";

export interface Collection {
  id: string;
  name: string;
  count: number;
}

export interface DayTotal {
  day: string;
  seconds: number;
}

export interface ReadingStats {
  todaySeconds: number;
  weekSeconds: number;
  totalSeconds: number;
  streakDays: number;
  finishedThisYear: number;
  booksFinished: number;
  booksReading: number;
  daily: DayTotal[];
}

export interface BookStats {
  seconds: number;
  daysRead: number;
  firstDay: string | null;
  lastDay: string | null;
  progressRead: number;
  secondsLeft: number | null;
}

export type SortKey = "title" | "author" | "series" | "recent" | "progress" | "added";

export interface BookQuery {
  search?: string | null;
  sort: SortKey;
  descending: boolean;
  status?: BookStatus | null;
  collection?: string | null;
}

export interface Progress {
  locator: string;
  progression: number;
  updatedAt: number;
}

export interface OpenedBook {
  book: BookSummary;
  manifest: Manifest;
  progress: Progress | null;
}

export type ImportOutcome =
  | { status: "imported"; book: BookSummary; warnings: string[] }
  | { status: "duplicate"; book: BookSummary };

export interface ImportResult {
  path: string;
  outcome: ImportOutcome | null;
  error: string | null;
  drm: boolean;
}

export type AnnotationKind = "highlight" | "note" | "bookmark";

export interface Annotation {
  id: string;
  bookId: string;
  kind: AnnotationKind;
  start: string;
  end: string | null;
  quote: string;
  note: string;
  color: string;
  chapter: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface NewAnnotation {
  bookId: string;
  kind: AnnotationKind;
  start: string;
  end?: string | null;
  quote?: string;
  note?: string;
  color?: string;
  chapter?: string | null;
}

/** A book's highlights, notes and bookmarks as Markdown (Phase 7). */
export interface ExportedAnnotations {
  title: string;
  fileName: string;
  markdown: string;
  /** The Rust side can open a native share sheet (Android). */
  nativeShare: boolean;
}
