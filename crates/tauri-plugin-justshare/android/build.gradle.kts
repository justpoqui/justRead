// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0
// Android library for tauri-plugin-justshare (standard Tauri plugin layout).

import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.justread.share"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

dependencies {
    // FileProvider lives in androidx.core (Apache-2.0).
    implementation("androidx.core:core-ktx:1.9.0")
    implementation(project(":tauri-android"))
}
