# ADR 0008 — justBrowse architecture

**Status:** withdrawn — justBrowse was removed from justRead at the owner's request (its place in the
library header became the Profile page). Kept for the record of what was built and learned.

## Context
justBrowse is a built-in browser for finding free books. Requirements: system webview per platform,
zero IPC for web content, no access to the reader's storage, HTTPS-only (warn before HTTP), tracker/ad
blocklist, no third-party cookies, private sessions that clear on close, no autofill/password storage,
configurable search (DuckDuckGo default, SearXNG possible), book downloads go to the library.

## Findings (Tauri 2.12.5 source, verified)
* `Window::add_child` (several webviews in one window) is **desktop-only and behind the `unstable`
  feature**; it does not exist on Android/iOS.
* `WebviewBuilder::proxy_url` works on Linux and Windows; macOS needs the `macos-proxy` feature and 14+.
* `incognito`, `on_navigation`, `on_new_window`, `on_download`, `general_autofill_enabled` (Windows) exist.
* On Linux, child webviews are packed into the window's `gtk::Box`; `set_bounds` is ignored there, so the
  layout must be set through GTK packing (found by an end-to-end screenshot, now asserted by a test).
* WebDriver (WebKitWebDriver) exposes only one webview per window: the toolbar can be driven, the content
  cannot, so the content's isolation is tested from *inside* the page (it reports to a local test server).

## Decision (desktop)
One window, two webviews:
* **`browse-chrome`**: our local toolbar page, with a capability granting only `browse_*` commands and
  event listening.
* **`browse-content`**: remote pages. **No capability names it** (and capabilities don't cover remote
  origins anyway), so every IPC call is rejected by the ACL. Incognito (ephemeral, cleared on close),
  autofill off (Windows), third-party cookies refused (set explicitly on Linux; WebKit default elsewhere),
  popups opened in place, top-level navigation policy (https allowed, http only after an explicit
  warning, file:/data:/javascript:/app schemes never).
* **Local proxy** (`browse/proxy.rs`) for *all* content traffic: blocklist on every request, HTTPS
  tunnelled without decryption (host names only), plain HTTP refused unless allowed, loopback/link-local
  destinations refused after DNS resolution (DNS-rebinding safe), only ports 443/8443 tunnelled.
* `justbook://` answers only the main reader webview.
* Downloads go to a private cache folder, are imported if they are books, and are deleted afterwards.

## Consequences / risks
* Depends on Tauri's `unstable` multi-webview API on desktop: may change between Tauri releases.
* macOS < 14: no proxy, so no blocklist and no HTTPS-only enforcement for sub-resources (top-level
  navigation rules still apply).
* Windows: third-party cookie behaviour relies on WebView2's InPrivate session; to be verified on Windows.
* The bundled blocklist is an original, modest list (community lists are GPL/CC-BY-SA); domain-level only.
* Mobile (Phase 6): a native browser screen (WKWebView / Android WebView) with its own native toolbar,
  outside the Tauri webview, provided by a small plugin — no IPC bridge at all.
* Never described as "safe" or "secure" in absolute terms: the UI says "privacy-hardened".
