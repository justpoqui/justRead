// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// A minimal in-memory stand-in for the Rust commands (src-tauri/src/commands.rs).
// It records calls on `window.__jr` so the test script can assert on them.

import type { Annotation, BookStatus, BookSummary, Collection, Manifest } from "../src/lib/ipc/types";

interface State {
  manifests: Map<string, Manifest>;
  progress: Map<string, { locator: string; progression: number; updatedAt: number }>;
  settings: Map<string, unknown>;
  annotations: Annotation[];
  calls: { cmd: string; args: unknown }[];
  // Phase 8
  shelf: Map<string, { status: BookStatus; finishedAt: number | null; lastOpenedAt: number | null; collections: Set<string> }>;
  collections: Collection[];
  reading: { bookId: string; day: string; seconds: number; progress: number }[];
}

declare global {
  interface Window {
    __jr: State;
  }
}

export async function installBackend() {
  // STEP 1: load every package the dev server exposes.
  const ids: string[] = await (await fetch("/__books/ids.json")).json();
  const manifests = new Map<string, Manifest>();
  for (const id of ids) manifests.set(id, await (await fetch(`/__books/${id}/manifest.json`)).json());

  const state: State = {
    manifests, progress: new Map(), settings: new Map(), annotations: [], calls: [],
    shelf: new Map(), collections: [], reading: [],
  };
  const shelfOf = (id: string) => {
    if (!state.shelf.has(id)) state.shelf.set(id, { status: "unread", finishedAt: null, lastOpenedAt: null, collections: new Set() });
    return state.shelf.get(id)!;
  };
  window.__jr = state;

  const summary = (m: Manifest): BookSummary => ({
    id: m.id,
    title: m.metadata.title,
    authors: m.metadata.authors,
    series: m.metadata.series,
    seriesIndex: m.metadata.seriesIndex,
    language: m.metadata.language,
    format: m.source.format,
    cover: m.cover,
    addedAt: 0,
    lastOpenedAt: shelfOf(m.id).lastOpenedAt,
    progression: state.progress.get(m.id)?.progression ?? 0,
    status: shelfOf(m.id).status,
    finishedAt: shelfOf(m.id).finishedAt,
    collections: [...shelfOf(m.id).collections],
  });

  let nextId = 1;
  const handle = async (cmd: string, args: Record<string, unknown>): Promise<unknown> => {
    state.calls.push({ cmd, args });
    switch (cmd) {
      case "supported_extensions":
        return ["epub", "txt"];
      case "list_books": {
        const q = args.query as { status?: BookStatus | null; collection?: string | null; sort?: string };
        let list = [...manifests.values()].map(summary);
        if (q?.status) list = list.filter((b) => b.status === q.status);
        if (q?.collection) list = list.filter((b) => b.collections.includes(q.collection!));
        if (q?.sort === "recent") list.sort((a, b) => (b.lastOpenedAt ?? 0) - (a.lastOpenedAt ?? 0));
        return list;
      }
      case "open_book": {
        const m = manifests.get(args.id as string)!;
        const sh = shelfOf(m.id);
        sh.lastOpenedAt = Date.now();
        if (sh.status === "unread") sh.status = "reading";
        return { book: summary(m), manifest: m, progress: state.progress.get(m.id) ?? null };
      }
      case "read_chapter":
        return (await fetch(`/__books/${args.bookId}/${args.href}`)).text();
      case "save_progress":
        state.progress.set(args.bookId as string, { locator: args.locator as string, progression: args.progression as number, updatedAt: Date.now() });
        return null;
      case "get_setting":
        return state.settings.get(args.key as string) ?? null;
      case "set_setting":
        state.settings.set(args.key as string, args.value);
        return null;
      case "list_annotations":
        return state.annotations.filter((a) => a.bookId === args.bookId);
      case "add_annotation": {
        const n = args.annotation as Annotation;
        const a: Annotation = { ...n, id: `a${nextId++}`, end: n.end ?? null, note: n.note ?? "", color: n.color ?? "yellow", quote: n.quote ?? "", chapter: n.chapter ?? null, createdAt: Date.now(), updatedAt: Date.now() };
        state.annotations.push(a);
        return a;
      }
      case "update_annotation": {
        const a = state.annotations.find((x) => x.id === args.id)!;
        Object.assign(a, args.patch);
        return a;
      }
      // Phase 7 export: a simplified document is enough for the UI flow.
      case "export_annotations": {
        const list = state.annotations.filter((a) => a.bookId === args.bookId);
        const markdown = `# Highlights\n\n${list.map((a) => `> ${a.quote}${a.note ? `\n\n**Note:** ${a.note}` : ""}`).join("\n\n")}\n`;
        return { title: "Book — highlights", fileName: "Book — highlights.md", markdown, nativeShare: /Android/i.test(navigator.userAgent) };
      }
      case "share_annotations":
        return null;
      case "save_annotations":
        return "Book — highlights.md";
      // Phase 8: shelves, collections, reading time.
      case "set_book_status": {
        const sh = shelfOf(args.bookId as string);
        sh.status = args.status as BookStatus;
        sh.finishedAt = sh.status === "finished" ? Date.now() : null;
        return null;
      }
      case "list_collections":
        return state.collections.map((c) => ({ ...c, count: [...state.shelf.values()].filter((s) => s.collections.has(c.id)).length }));
      case "create_collection": {
        const name = (args.name as string).trim();
        if (state.collections.some((c) => c.name.toLowerCase() === name.toLowerCase())) throw `There is already a collection called “${name}”.`;
        const c = { id: `c${nextId++}`, name, count: 0 };
        state.collections.push(c);
        return c;
      }
      case "rename_collection":
        state.collections.find((c) => c.id === args.id)!.name = args.name as string;
        return null;
      case "delete_collection":
        state.collections = state.collections.filter((c) => c.id !== args.id);
        for (const s of state.shelf.values()) s.collections.delete(args.id as string);
        return null;
      case "set_in_collection": {
        const sh = shelfOf(args.bookId as string);
        if (args.member) sh.collections.add(args.collectionId as string);
        else sh.collections.delete(args.collectionId as string);
        return null;
      }
      case "record_reading":
        state.reading.push({ bookId: args.bookId as string, day: args.day as string, seconds: args.seconds as number, progress: args.progress as number });
        return null;
      case "reading_stats": {
        // Simplified: per-day totals for the chart and tiles.
        const today = args.today as string;
        const days = args.days as number;
        const per = new Map<string, number>();
        for (const r of state.reading) per.set(r.day, (per.get(r.day) ?? 0) + r.seconds);
        const t = new Date(today + "T12:00:00");
        const daily = Array.from({ length: days }, (_, i) => {
          const d = new Date(t);
          d.setDate(t.getDate() - (days - 1 - i));
          const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
          return { day, seconds: per.get(day) ?? 0 };
        });
        const total = [...per.values()].reduce((a, b) => a + b, 0);
        const shelves = [...state.shelf.values()];
        return {
          todaySeconds: per.get(today) ?? 0,
          weekSeconds: daily.slice(-7).reduce((a, d) => a + d.seconds, 0),
          totalSeconds: total,
          streakDays: (per.get(today) ?? 0) > 0 ? 1 : 0,
          finishedThisYear: shelves.filter((s) => s.status === "finished").length,
          booksFinished: shelves.filter((s) => s.status === "finished").length,
          booksReading: shelves.filter((s) => s.status === "reading").length,
          daily,
        };
      }
      case "book_stats": {
        const rows = state.reading.filter((r) => r.bookId === args.bookId);
        return { seconds: rows.reduce((a, r) => a + r.seconds, 0), daysRead: new Set(rows.map((r) => r.day)).size, firstDay: null, lastDay: null, progressRead: 0, secondsLeft: null };
      }
      case "set_screen":
        return null;
      case "take_pending_open":
        return [];
      case "set_cover":
        // Recorded so the test can check that a real JPEG was rendered. The mock
        // has nowhere to store it, so report "no cover" (the UI keeps its placeholder).
        return null;
      case "delete_annotation":
        state.annotations = state.annotations.filter((x) => x.id !== args.id);
        return null;
      default:
        // Window/event plugin calls (set_title, listen, …) are no-ops here.
        return null;
    }
  };
  return { handle };
}
