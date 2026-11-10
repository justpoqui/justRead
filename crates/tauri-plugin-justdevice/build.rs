// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Registers the Android library (`android/`) with the app build.
//! The plugin exposes no JavaScript commands: the app's own Rust command
//! calls it, so the webview never gets a direct "share anything" API.
//! Docs: <https://v2.tauri.app/develop/plugins/develop-mobile/>

fn main() {
    tauri_plugin::Builder::new(&[])
        .android_path("android")
        .build();
}
