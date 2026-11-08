# ADR 0003 — Chapter isolation: CSP meta instead of iframe sandbox

**Status:** accepted (Phase 2)

## Context
Chapters render in `srcdoc` iframes that are same-origin with the app, so the engine can measure
layout, read selections and paint highlights. The app origin has IPC access, so book content must
never execute script.

The first implementation used `<iframe sandbox="allow-same-origin">` (no `allow-scripts`). It worked in
Chromium, but the WebKitGTK end-to-end run showed that **WebKit does not invoke even the parent's event
listeners** in such a frame. A probe in the real app (WebDriver, native clicks) logged events for an
unsandboxed frame and none for the sandboxed one. Selection, link handling and page taps were dead on
WebKitGTK — and would be on Safari/WKWebView (macOS, iOS) too. Adding `allow-scripts` alongside
`allow-same-origin` is documented as making the sandbox escapable, so that is no fix.

## Decision
No `sandbox` attribute. Two independent barriers instead:

1. **Import-time sanitizing** (`crates/justread-formats/src/html.rs`): `<script>`, `on*` attributes,
   `javascript:` URLs, forms, objects and embeds are removed; only allow-listed elements survive.
2. **Per-chapter CSP**: `buildSrcdoc` puts
   `<meta http-equiv="Content-Security-Policy" content="script-src 'none'; object-src 'none'; frame-src 'none'; form-action 'none'">`
   first in every chapter `<head>`.

A second probe confirmed, in both Chromium and WebKitGTK, that with this setup an inline `<script>`,
an `onclick` attribute and an `onerror` handler all stay blocked while the parent's listeners fire.
The app-level CSP (tauri.conf.json) and the deny-by-default command ACL remain as further layers.

## Consequences
* Interaction works identically on all engines (verified by `ui/e2e/run.mjs` and `ui/e2e/webkit.mjs`).
* Anything that renders chapters must go through `buildSrcdoc`; a unit test asserts the CSP comes first.
