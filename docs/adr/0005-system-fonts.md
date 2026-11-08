# ADR 0005 — System fonts only (no bundled fonts) for now

**Status:** accepted (Phase 1)

Good open reading fonts (Literata, Atkinson Hyperlegible, …) are licensed under the SIL OFL-1.1,
which is permissive for fonts but outside the project's dependency license list. Until that is
explicitly approved, justRead uses platform font stacks (serif, sans, humanist, mono) and lets users
type the name of any font installed on their system. Nothing is downloaded (offline-only).
