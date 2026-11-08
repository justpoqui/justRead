// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Small transient notifications ("3 books imported", "DRM-protected…").

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "error";
}

export const toasts: Toast[] = $state([]);
let next = 1;

export function toast(text: string, kind: Toast["kind"] = "info", ms = 4500): void {
  const id = next++;
  toasts.push({ id, text, kind });
  setTimeout(() => dismiss(id), ms);
}

export function dismiss(id: number): void {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}
