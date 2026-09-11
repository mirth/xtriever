#!/usr/bin/env bash
# Build the iOS harness inputs: staticlibs for both slices, Swift bindings, and an XCFramework.
#
# Everything this produces is generated and gitignored. FR-031 makes the harness a *durable*
# deliverable, which means reproducible from source rather than committed as binaries — the
# XCFramework alone is ~262 MB.
#
#     scripts/build-ios-harness.sh [--debug]
#
# Prerequisite: scripts/check-toolchain.sh must pass. A cross-target build recorded without it is
# void (research D14).

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

profile="release"
profile_flag="--release"
with_model=false
for arg in "$@"; do
    case "$arg" in
        --debug)      profile="debug"; profile_flag="" ;;
        --with-model) with_model=true ;;
        *) echo "unknown option: $arg" >&2; exit 1 ;;
    esac
done

harness="$repo_root/harness/ios/XtrieverSpike"
frameworks="$harness/Frameworks"
generated="$harness/Sources/XtrieverSpike/Generated"
staging="$repo_root/build/swift"

"$repo_root/scripts/check-toolchain.sh"

echo "==> building staticlibs ($profile)"
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
    # shellcheck disable=SC2086  # profile_flag is intentionally word-split (it may be empty)
    cargo build -q -p xtriever-ffi --features spike $profile_flag --target "$target"
    printf '    %-24s %s\n' "$target" \
        "$(ls -lh "target/$target/$profile/libxtriever_ffi.a" | awk '{print $5}')"
done

device_lib="target/aarch64-apple-ios/$profile/libxtriever_ffi.a"

echo "==> generating Swift bindings"
rm -rf "$staging" "$generated"
mkdir -p "$staging/Headers" "$staging/Modules" "$generated"
bindgen() { cargo run -q -p xtriever-ffi --features cli --bin uniffi-bindgen -- "$@"; }
bindgen "$device_lib" "$generated" --swift-sources
bindgen "$device_lib" "$staging/Headers" --headers
# Two flags, both load-bearing, and one flag deliberately NOT passed:
#   --module-name xtriever_ffiFFI
#                      the generated Swift opens with `#if canImport(xtriever_ffiFFI)`. Without
#                      this the modulemap declares `xtriever_ffi`, `canImport` is quietly false,
#                      the import is skipped, and every FFI type (`RustBuffer`, `ForeignBytes`, …)
#                      is "not in scope" — a confusing failure a long way from its cause.
#   --modulemap-filename module.modulemap
#                      Clang and XCFrameworks only look for that exact filename.
#   NOT --xcframework  despite the name. It emits `framework module`, which requires a real
#                      .framework layout; our slices are static libraries (`libxtriever_ffi.a`),
#                      so Clang never matches the module and you get the same "not in scope" wall.
#                      Verified empirically — see report.md F-006.
bindgen "$device_lib" "$staging/Modules" --modulemap \
        --module-name xtriever_ffiFFI --modulemap-filename module.modulemap

# The headers directory handed to -create-xcframework must carry both the header and the modulemap,
# or Swift cannot see the module.
headers="$repo_root/build/xcf-headers"
rm -rf "$headers"; mkdir -p "$headers"
cp "$staging/Headers/xtriever_ffiFFI.h" "$staging/Modules/module.modulemap" "$headers/"

echo "==> assembling XCFramework"
mkdir -p "$frameworks"
rm -rf "$frameworks/XtrieverFFI.xcframework"
xcodebuild -create-xcframework \
    -library "target/aarch64-apple-ios/$profile/libxtriever_ffi.a"     -headers "$headers" \
    -library "target/aarch64-apple-ios-sim/$profile/libxtriever_ffi.a" -headers "$headers" \
    -output "$frameworks/XtrieverFFI.xcframework" >/dev/null

echo "==> staging test resources"
resources="$harness/Sources/XtrieverSpike/XtrieverData"
rm -rf "$resources"; mkdir -p "$resources/fixtures"
cp "$repo_root"/reference/fixtures/001/*.json "$resources/fixtures/"
if [ "$with_model" = true ]; then
    # 87.1 MiB. Only staged on request: a device run needs it bundled (there is no host filesystem
    # to read from), a simulator run does not, and copying it every build is pure waste.
    mkdir -p "$resources/model"
    cp "$repo_root"/reference/models/001/config.json \
       "$repo_root"/reference/models/001/tokenizer.json \
       "$repo_root"/reference/models/001/model.safetensors "$resources/model/"
    printf '    model bundled (%s)\n' "$(du -sh "$resources/model" | cut -f1)"
else
    printf '    model NOT bundled — pass --with-model for a device run\n'
fi

echo "==> generating the host app project"
# A physical device cannot run a hostless XCTest bundle ("Tool-hosted testing is unavailable on
# device destinations"), so the tests need an application to host them. The .xcodeproj is derived,
# not authored — it is regenerated here and gitignored.
if command -v xcodegen >/dev/null 2>&1; then
    (cd "$repo_root/harness/ios/XtrieverSpikeApp" && xcodegen generate >/dev/null)
    printf '    XtrieverSpikeApp.xcodeproj regenerated\n'
    app_project=true
else
    app_project=false
fi

if [ "$app_project" = false ]; then
    # Reporting PASS here would let a caller believe the harness is ready and only discover the
    # missing host project during the device run itself.
    printf 'build-ios-harness: INCOMPLETE — xcodegen is not installed, so the host app project was\n' >&2
    printf '  not generated. The SIMULATOR path works; a DEVICE run cannot (a device rejects a\n' >&2
    printf '  hostless XCTest bundle). Install it with `brew install xcodegen` and re-run.\n' >&2
    exit 2
fi

echo "build-ios-harness: PASS"
printf '    xcframework %s  (%s)\n' "$frameworks/XtrieverFFI.xcframework" \
    "$(du -sh "$frameworks/XtrieverFFI.xcframework" | cut -f1)"
printf '    bindings    %s\n' "$generated/xtriever_ffi.swift"
printf '    next: see harness/ios/DEVICE-RUN.md\n'
