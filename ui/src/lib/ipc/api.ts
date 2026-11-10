// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Typed wrappers around Tauri `invoke`. Every backend call goes through
// here, so the rest of the UI never deals with command names or argument
// casing. Docs: https://v2.tauri.app/develop/calling-rust/

import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type {
  Annotation,
  BookQuery,
  BookSummary,
  ImportResult,
  NewAnnotation,
  OpenedBook,
  ExportedAnnotations,
  BookStatus,
  Collection,
  ReadingStats,
  BookStats,
} from "./types";

export const api = {
  supportedExtensions: () => invoke<string[]>("supported_extensions"),
  listBooks: (query: BookQuery) => invoke<BookSummary[]>("list_books", { query }),
  getBook: (id: string) => invoke<BookSummary>("get_book", { id }),
  importBooks: (paths: string[]) => invoke<ImportResult[]>("import_books", { paths }),
  deleteBook: (id: string) => invoke<void>("delete_book", { id }),
  openBook: (id: string) => invoke<OpenedBook>("open_book", { id }),
  readChapter: (bookId: string, href: string) => invoke<string>("read_chapter", { bookId, href }),
  saveProgress: (bookId: string, locator: string, progression: number) =>
    invoke<void>("save_progress", { bookId, locator, progression }),
  getSetting: <T>(key: string) => invoke<T | null>("get_setting", { key }),
  setSetting: (key: string, value: unknown) => invoke<void>("set_setting", { key, value }),
  listAnnotations: (bookId: string) => invoke<Annotation[]>("list_annotations", { bookId }),
  addAnnotation: (annotation: NewAnnotation) => invoke<Annotation>("add_annotation", { annotation }),
  updateAnnotation: (id: string, patch: { note?: string; color?: string }) =>
    invoke<Annotation>("update_annotation", { id, patch }),
  deleteAnnotation: (id: string) => invoke<void>("delete_annotation", { id }),
  // Phase 7: highlights as Markdown, for any app.
  exportAnnotations: (bookId: string) => invoke<ExportedAnnotations>("export_annotations", { bookId }),
  shareAnnotations: (bookId: string) => invoke<void>("share_annotations", { bookId }),
  saveAnnotations: (bookId: string) => invoke<string | null>("save_annotations", { bookId }),
  // Phase 8: shelves, collections, reading time and stats.
  setBookStatus: (bookId: string, status: BookStatus) => invoke<void>("set_book_status", { bookId, status }),
  listCollections: () => invoke<Collection[]>("list_collections"),
  createCollection: (name: string) => invoke<Collection>("create_collection", { name }),
  renameCollection: (id: string, name: string) => invoke<void>("rename_collection", { id, name }),
  deleteCollection: (id: string) => invoke<void>("delete_collection", { id }),
  setInCollection: (bookId: string, collectionId: string, member: boolean) =>
    invoke<void>("set_in_collection", { bookId, collectionId, member }),
  recordReading: (bookId: string, day: string, seconds: number, progress: number) =>
    invoke<void>("record_reading", { bookId, day, seconds, progress }),
  readingStats: (today: string, days: number) => invoke<ReadingStats>("reading_stats", { today, days }),
  bookStats: (bookId: string) => invoke<BookStats>("book_stats", { bookId }),
  // Reader screen timeout (native on Android; a no-op elsewhere).
  setScreen: (keepOn: boolean, dim: boolean) => invoke<void>("set_screen", { keepOn, dim }),
  openBrowser: () => invoke<void>("open_browser"),
  takePendingOpen: () => invoke<string[]>("take_pending_open"),
  // Bytes travel as a number array; covers are small (~30 KB JPEG).
  setCover: (bookId: string, bytes: Uint8Array) => invoke<string>("set_cover", { bookId, bytes: Array.from(bytes) }),
};

/**
 * Base URL of the `justbook://` protocol on this platform.
 *
 * Tauri serves custom schemes as `justbook://localhost/` on macOS/Linux and
 * `http://justbook.localhost/` on Windows/Android. `convertFileSrc` knows
 * which, but it percent-encodes "/" — fine for one file, wrong for a
 * `<base href>` that relative URLs resolve against. So we take only its
 * prefix and encode path segments ourselves.
 */
export function packageUrl(bookId: string, path: string): string {
  // STEP 1: discover the platform prefix from a dummy conversion.
  const probe = convertFileSrc("x", "justbook");
  const prefix = probe.slice(0, probe.length - 1);
  // STEP 2: encode each segment, keep the slashes.
  const encoded = `${bookId}/${path}`.split("/").map(encodeURIComponent).join("/");
  return prefix + encoded;
}
