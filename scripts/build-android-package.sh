#!/usr/bin/env bash
# Build the Xtriever Android library module: the native library for 64-bit ARM, the uniffi
# Kotlin bindings generated from that same library, and (on request) the staged resources the
# instrumented tests and the demonstration need.
#
# Everything this produces is generated and gitignored — reproducible from source rather than
# committed (Feature 025, spec FR-003). It is the Android counterpart of
# scripts/build-ios-package.sh and follows its shape deliberately.
#
#     scripts/build-android-package.sh [--debug] [--with-fixtures] [--with-models] [--with-corpus]
#
#   --with-fixtures  build the 40-document parity fixture index + goldens and stage them for the
#                    module's instrumented test (the oracle: swift/Xtriever/Tests/Fixtures)
#   --with-models    stage both pinned models into the demo's assets (~174 MB)
#   --with-corpus    stage the 2,000-article Wikipedia slice into the demo's assets (~24 MB)
#
# Two hard requirements, both measured before this script existed (research D2, D4):
#   * the native build needs half precision enabled — the inference engine's f16 matrix kernel
#     writes inline assembly the base aarch64 profile rejects ("instruction requires: fullfp16");
#   * the library must keep its symbols, because uniffi's metadata lives in the symbol table and
#     the release profile strips it — hence [profile.android] in Cargo.toml.
# Both are asserted below rather than trusted.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

profile="android"
profile_flag="--profile android"
with_fixtures=false
with_models=false
with_corpus=false
for arg in "$@"; do
    case "$arg" in
        --debug)         profile="debug"; profile_flag="" ;;
        --with-fixtures) with_fixtures=true ;;
        --with-models)   with_models=true ;;
        --with-corpus)   with_corpus=true ;;
        *) echo "unknown option: $arg" >&2; exit 1 ;;
    esac
done

abi="arm64-v8a"
target="aarch64-linux-android"
module="$repo_root/android/xtriever"
jni_libs="$module/src/main/jniLibs/$abi"
generated="$module/src/main/kotlin"
demo_assets="$repo_root/apps/android-wiki-demo/src/main/assets"
fixture_dir="$module/src/androidTest/assets/fixture"
# Everything staged into an application package, budgeted as the iOS script budgets its bundle.
budget=400000000

"$repo_root/scripts/check-toolchain.sh"

# ── prerequisites the toolchain script does not cover ────────────────────────────────────────
if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo "cargo-ndk is not installed — cargo install cargo-ndk" >&2
    exit 1
fi
: "${ANDROID_NDK_HOME:=${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}/ndk/28.2.13676358}"
if [[ ! -d "$ANDROID_NDK_HOME" ]]; then
    echo "no NDK at $ANDROID_NDK_HOME — sdkmanager \"ndk;28.2.13676358\", or set ANDROID_NDK_HOME" >&2
    exit 1
fi
export ANDROID_NDK_HOME
ndk_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"

echo "==> building the native library ($profile, $abi)"
rm -rf "$jni_libs"
mkdir -p "$jni_libs"
# shellcheck disable=SC2086  # profile_flag is intentionally word-split.
# Note the order: everything after `build` goes to cargo, so --profile follows it.
RUSTFLAGS="-C target-feature=+fp16" cargo ndk -t "$abi" -o "$module/src/main/jniLibs" build $profile_flag -p xtriever-ffi
lib="$jni_libs/libxtriever_ffi.so"
[[ -f "$lib" ]] || { echo "no library at $lib" >&2; exit 1; }
printf '    %-28s %s\n' "$abi" "$(du -sh "$lib" | cut -f1)"

# ── the two assertions (research D2, D4) ─────────────────────────────────────────────────────
meta=$("$ndk_bin/llvm-nm" "$lib" 2>/dev/null | grep -c UNIFFI_META || true)
if [[ "$meta" -eq 0 ]]; then
    echo "the library carries no UNIFFI_META symbols — bindings cannot be generated from it." >&2
    echo "the build profile stripped them; Cargo.toml's [profile.android] must keep symbols." >&2
    exit 1
fi
# Every loadable segment, not just the first: one 4 KB-aligned segment later in the file is
# enough for Android 15 to refuse the library.
aligns=$("$ndk_bin/llvm-readelf" -l "$lib" 2>/dev/null | awk '/LOAD/ {print $NF}')
[[ -n "$aligns" ]] || { echo "no LOAD segments in $lib" >&2; exit 1; }
bad=$(echo "$aligns" | grep -vc '^0x4000$' || true)
if [[ "$bad" -ne 0 ]]; then
    echo "$bad of $(echo "$aligns" | wc -l | tr -d ' ') loadable segments are not aligned at 0x4000:" >&2
    echo "$aligns" | sort | uniq -c >&2
    echo "Android 15 requires 16 KB pages." >&2
    exit 1
fi
printf '    %-28s %s metadata symbols, %s loadable segments all aligned at 0x4000\n' \
    "checks" "$meta" "$(echo "$aligns" | wc -l | tr -d ' ')"

echo "==> generating the Kotlin bindings"
rm -rf "${generated:?}/uniffi"
cargo run -q -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
    generate --library "$lib" --language kotlin --out-dir "$generated" --no-format
kt=$(find "$generated/uniffi" -name '*.kt' | head -1)
[[ -n "$kt" ]] || { echo "no Kotlin generated under $generated/uniffi" >&2; exit 1; }

# uniffi 0.32.1 (the newest release on 2026-09-20) emits Kotlin that does not compile when an
# error variant carries a field called `message`, and seven of ours do: it declares
# `val message` in the constructor *and* `override val message` in the body, which collides
# with `Throwable.message`. The fix is the one the compiler asks for — mark the constructor
# property `override` and drop the redeclaration — applied only inside `XtrieverException`,
# because `DegradeReason.StageError` has the same field name and overrides nothing.
# Effect on the surface: `e.message` is the engine's text. For the one variant that also
# carries `model`, that text no longer repeats the model name, which stays a property.
# If a future uniffi stops emitting this shape, the count below is 0 and the build stops
# rather than shipping something unchecked.
python3 - "$kt" <<'PATCH'
import re, sys
from pathlib import Path
path = Path(sys.argv[1])
source = path.read_text()
at = source.index("sealed class XtrieverException")
head, block = source[:at], source[at:]
patched = block.replace("        val `message`: kotlin.String", "        override val `message`: kotlin.String")
patched = re.sub(r'\n        override val message\n            get\(\) = "[^"]*\$\{ `message` \}"\n', "\n", patched)
count = patched.count("override val `message`")
if count == 0:
    sys.exit("no `message` collision found in XtrieverException — has uniffi changed its Kotlin output?")
path.write_text(head + patched)
print(f"    {'bindings patched':<28} {count} error variants given the override Kotlin requires")
PATCH
printf '    %-28s %s lines\n' "$(basename "$kt")" "$(wc -l < "$kt" | tr -d ' ')"

staged=0
stage_dir() {  # stage_dir <source> <destination> <label>
    local src="$1" dest="$2" label="$3"
    [[ -d "$src" ]] || { echo "missing $label at $src" >&2; exit 1; }
    rm -rf "$dest"
    mkdir -p "$(dirname "$dest")"
    cp -R "$src" "$dest"
    local bytes
    bytes=$(du -sk "$dest" | cut -f1)
    staged=$((staged + bytes * 1024))
    printf '    %-28s %s\n' "$label" "$(du -sh "$dest" | cut -f1)"
}

if $with_fixtures; then
    echo "==> staging the parity fixture"
    if [[ ! -d "$repo_root/swift/Xtriever/Tests/Fixtures/index" ]]; then
        echo "no fixture index — run scripts/build-ios-package.sh --with-fixtures first" >&2
        exit 1
    fi
    stage_dir "$repo_root/swift/Xtriever/Tests/Fixtures/index" "$fixture_dir/index" "fixture index"
    mkdir -p "$fixture_dir"
    cp "$repo_root/swift/Xtriever/Tests/Fixtures/expected.json" "$fixture_dir/expected.json"
    printf '    %-28s %s\n' "goldens" "$(du -sh "$fixture_dir/expected.json" | cut -f1)"
    # Both models travel with the test package, as they do in the iOS package: it keeps
    # `./gradlew :android:xtriever:connectedAndroidTest` self-contained, with nothing to push
    # by hand before it runs.
    stage_dir "${XTRIEVER_MODEL_DIR:-$repo_root/reference/models/all-MiniLM-L6-v2}" "$fixture_dir/models/embedder" "test embedder"
    stage_dir "${XTRIEVER_RERANK_MODEL_DIR:-$repo_root/reference/models/ms-marco-MiniLM-L-6-v2}" "$fixture_dir/models/reranker" "test re-ranker"
fi

if $with_models; then
    echo "==> staging the models"
    stage_dir "${XTRIEVER_MODEL_DIR:-$repo_root/reference/models/all-MiniLM-L6-v2}" "$demo_assets/models/embedder" "embedder"
    stage_dir "${XTRIEVER_RERANK_MODEL_DIR:-$repo_root/reference/models/ms-marco-MiniLM-L-6-v2}" "$demo_assets/models/reranker" "re-ranker"
fi

if $with_corpus; then
    echo "==> staging the corpus slice"
    slice="${XTRIEVER_ANDROID_CORPUS:-$repo_root/target/xt-wiki-slice-py}"
    [[ -d "$slice/index" ]] || { echo "no corpus at $slice — wikidemo build --limit 2000 --out $slice" >&2; exit 1; }
    stage_dir "$slice/index" "$demo_assets/corpus/index" "corpus index"
    cp "$slice/ATTRIBUTION.txt" "$demo_assets/corpus/ATTRIBUTION.txt"
    cp "$slice/index/corpus.json" "$demo_assets/corpus/corpus.json" 2>/dev/null || true
fi

if [[ "$staged" -gt "$budget" ]]; then
    echo "staged $staged bytes, over the ${budget}-byte budget" >&2
    exit 1
fi
[[ "$staged" -gt 0 ]] && printf '    %-28s %s of the %s budget\n' "staged" "$staged bytes" "$budget bytes"

echo "build-android-package: PASS"
