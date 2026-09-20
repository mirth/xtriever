// Feature 025: the reusable library module. It carries the generated uniffi bindings, the
// native library for 64-bit ARM, and a thin wrapper mirroring the Swift package's — no
// capability the Swift surface does not already have (spec FR-001).
//
// Everything generated (the bindings under src/main/kotlin/uniffi/, the library under
// src/main/jniLibs/, the test fixture under src/androidTest/assets/) is produced by
// scripts/build-android-package.sh and is gitignored.

plugins {
    // The Android Gradle Plugin has carried Kotlin support since version 9.0 and refuses the
    // separate Kotlin plugin outright (it says so, with a link); one plugin is all this needs.
    alias(libs.plugins.android.library)
}

android {
    namespace = "dev.xtriever.android"
    compileSdk = 37

    defaultConfig {
        minSdk = 26
        // 64-bit ARM only: it covers devices and the emulator images that run on Apple silicon
        // (research D2). The library additionally requires ARMv8.2 half precision — see
        // DeviceSupport and docs/adr/0014-android-half-precision-floor.md.
        ndk { abiFilters += "arm64-v8a" }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging {
        // The engine memory-maps the model weights and the vectors; a compressed library in the
        // package would have to be unpacked first, and JNA loads it directly.
        jniLibs.useLegacyPackaging = false
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }

    buildTypes {
        release { isMinifyEnabled = false }
    }
}

dependencies {
    // The generated bindings call the native library through Java Native Access; the Android
    // archive variant is the one that carries what it needs on a device.
    api("${libs.jna.get()}@aar")
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
