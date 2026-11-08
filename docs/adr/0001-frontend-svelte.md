# ADR 0001 — Svelte 5 for the UI

**Status:** accepted (Phase 1)

## Context
The UI needs a reactive framework for the library, toolbars and panels. The reading view itself
manipulates iframe DOM directly (pagination, measurement, highlights) and must stay framework-free.

## Decision
Svelte 5 (runes) + TypeScript + Vite, as a plain SPA (no SvelteKit). The reader engine
(`ui/src/lib/reader`) is plain TypeScript with no framework imports.

## Reasons
* Compiled output, small runtime: good for the mobile webviews of Phase 6.
* Runes (`$state`, `$derived`, `$effect`) are explicit and readable.
* Supported by `create-tauri-app` and documented by Tauri (<https://v2.tauri.app/start/frontend/>).
* MIT licensed.

## Alternatives
React (heavier runtime; VDOM adds nothing for direct-DOM reading code), SolidJS (close second;
smaller ecosystem).
