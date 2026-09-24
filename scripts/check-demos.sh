#!/usr/bin/env bash
# Build every demonstration against the engine as it stands, and run what needs no device.
#
# Why this exists: the demos consume the engine's public surfaces (the Python package, the Swift
# package, the Kotlin library) and some of them construct the engine's records themselves. A
# record that gains a field compiles in the engine and its bindings and still breaks a demo:
# Feature 027 added `StageReport.sparseSkipped`, and the Android demo — which builds an empty
# report of its own — stopped compiling while every library gate passed. This script is the
# gate step that catches that (CLAUDE.md, "The local gate").
#
#     scripts/check-demos.sh [python] [ios] [android]      # default: all three
#
#   python   the fresh wheel into apps/python-wiki-demo/.venv, then both Python demos' tests
#            (the models and target/xt-wiki present; tests that need them skip otherwise)
#   ios      the Swift package and the demo project regenerated, then the demo app and its test
#            target compiled for the simulator (build-for-testing; no device, no simulator run)
#   android  the Kotlin library regenerated, then the demo app and its instrumented tests
#            compiled (no emulator)
#
# Prints one PASS/FAIL line per demo and exits 1 if any failed. Device and emulator runs, and
# the measured runs, stay in each demo's README.

set -uo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"

targets=("$@")
[ ${#targets[@]} -gt 0 ] || targets=(python ios android)

status=0
report() { # name, exit status
    if [ "$2" -eq 0 ]; then
        printf 'check-demos: %s PASS\n' "$1"
    else
        printf 'check-demos: %s FAIL (exit %s)\n' "$1" "$2"
        status=1
    fi
}

check_python() {
    local venv=apps/python-wiki-demo/.venv
    [ -x "$venv/bin/python" ] || {
        echo "check-demos: $venv missing — see apps/python-wiki-demo/README.md, Install" >&2
        return 1
    }
    rm -f target/wheels/xtriever-*.whl
    (cd python && .venv/bin/maturin build --release) \
        && uv pip install --python "$venv/bin/python" --force-reinstall target/wheels/xtriever-*.whl \
        && (cd apps/python-wiki-demo && .venv/bin/pytest -q) \
        && (cd apps/python-minimal-demo && ../python-wiki-demo/.venv/bin/pytest -q)
}

check_ios() {
    scripts/build-ios-package.sh --with-models --with-fixtures --demo \
        && (cd apps/ios-wiki-demo && xcodebuild build-for-testing -project XtrieverWikiDemo.xcodeproj \
            -scheme XtrieverWikiDemo -destination 'generic/platform=iOS Simulator' \
            -configuration Debug ARCHS=arm64 -quiet)
}

check_android() {
    scripts/build-android-package.sh --with-fixtures \
        && ./gradlew :apps:android-wiki-demo:compileDebugKotlin \
            :apps:android-wiki-demo:compileDebugAndroidTestKotlin
}

for target in "${targets[@]}"; do
    case "$target" in
        python) check_python; report python $? ;;
        ios) check_ios; report ios $? ;;
        android) check_android; report android $? ;;
        *) echo "check-demos: unknown demo '$target' (python, ios, android)" >&2; status=1 ;;
    esac
done
exit $status
