# ADR 0002 — Normalize on import into justBook

**Status:** accepted (Phase 1)

## Decision
Every reflowable format is converted once at import into justBook (docs/JUSTBOOK_SPEC.md). The
reader renders only justBook (and, from Phase 3, page images/PDF).

## Consequences
* One layout engine, one locator system, one highlight implementation for all reflowable formats.
* Untrusted markup is sanitized once, at a single choke point (`html::normalize` / `ChapterWriter`).
* Stable block ids can be stamped at import, which makes locators simple and robust (ADR 0004).
* Imports take longer than "open in place"; the original file is kept so packages can be
  regenerated when the converter improves (`source.converterVersion`).
