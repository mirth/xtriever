// Feature 025: the Android side of the repository. The Rust workspace is unaffected — Gradle
// and Cargo share the root directory and nothing else. `android/xtriever` is the reusable
// library module; `apps/android-wiki-demo` is the demonstration, beside the iOS and Python ones.

pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "xtriever-android"

include(":android:xtriever")
include(":apps:android-wiki-demo")
