// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// A minimal in-memory stand-in for the Rust commands (src-tauri/src/commands.rs).
// It records calls on `window.__jr` so the test script can assert on them.

import type { Annotation, BookSummary, Manifest } from "../src/lib/ipc/types";

interface State {
  manifests: Map<string, Manifest>;
  progress: Map<string, { locator: string; progression: number; updatedAt: number }>;
  settings: Map<string, unknown>;
  annotations: Annotation[];
  calls: { cmd: string; args: unknown }[];
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

  const state: State = { manifests, progress: new Map(), settings: new Map(), annotations: [], calls: [] };
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
    lastOpenedAt: null,
    progression: state.progress.get(m.id)?.progression ?? 0,
  });

  let nextId = 1;
  const handle = async (cmd: string, args: Record<string, unknown>): Promise<unknown> => {
    state.calls.push({ cmd, args });
    switch (cmd) {
      case "supported_extensions":
        return ["epub", "txt"];
      case "list_books":
        return [...manifests.values()].map(summary);
      case "open_book": {
        const m = manifests.get(args.id as string)!;
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
