// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Platform hints for the UI (layout only — never for security decisions).

/** Phones and tablets (Android, iOS/iPadOS). */
export const isMobile: boolean =
  typeof navigator !== "undefined" &&
  (/Android|iPhone|iPad|iPod/i.test(navigator.userAgent) ||
    // iPadOS reports itself as a Mac; touch support gives it away.
    (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1));
