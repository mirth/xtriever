// Feature 025: plugins are declared here and applied in the modules, the layout the Android
// tooling generates. Nothing is configured at the root.

plugins {
    // Kotlin support is built into the Android Gradle Plugin since 9.0; only the Compose
    // compiler plugin is still declared separately, and only the application module applies it.
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
