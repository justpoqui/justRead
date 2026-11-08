// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    justread_app_lib::run();
}
