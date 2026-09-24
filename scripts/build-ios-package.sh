#!/usr/bin/env bash
# Build the Xtriever Swift package: staticlibs for both iOS slices, the uniffi Swift bindings,
# an XCFramework, and (on request) the staged resources and the device harness project.
#
# Everything this produces is generated and gitignored: reproducible from source rather than
# committed as binaries (the XCFramework alone is ~260 MB; the bindings are generated from the
# Rust; the resources are pinned elsewhere). Feature 007 — replaces the 001 spike's
# build-ios-harness.sh and encodes the traps that spike recorded (001 report F-004–F-008).
#
#     scripts/build-ios-package.sh [--debug] [--with-models] [--with-fixtures] [--with-scifact] [--with-wiki|--with-wiki-slice|--with-wiki-dev] [--app] [--demo]
#
#   --with-models    stage both pinned models into the library bundle (~51.5 MB eight-bit, Feature 026; needed by every
#                    Swift test and by any device run — a device has no host filesystem)
#   --with-fixtures  build the 40-document parity fixture index + goldens and stage them
#   --with-scifact   stage the SciFact hybrid-rerank index and the measurement queries/truth
#   --with-wiki      stage the Feature 008 Wikipedia index from target/xt-wiki (index, build
#                    record, attribution, queries, host goldens); fails over the bundle budget
#   --with-wiki-slice  the same from target/xt-wiki-slice, a --limit build (the README's is the
#                    first 3922 articles, 19,998 passages) — a device run without the whole edition
#   --with-wiki-dev  the same from target/xt-wiki-dev (a --limit build) — simulator work only
#   --app            regenerate swift/XtrieverHarnessApp/*.xcodeproj (needs xcodegen)
#   --demo           regenerate apps/ios-wiki-demo/*.xcodeproj — the Feature 009 demo app (needs xcodegen)
#
# Prerequisite: scripts/check-toolchain.sh must pass. A cross-target build recorded without it
# is void (001 research D14).

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

profile="release"
profile_flag="--release"
with_models=false
with_fixtures=false
with_scifact=false
with_wiki=""
with_app=false
with_demo=false
for arg in "$@"; do
    case "$arg" in
        --debug)         profile="debug"; profile_flag="" ;;
        --with-models)   with_models=true ;;
        --with-fixtures) with_fixtures=true ;;
        --with-scifact)  with_scifact=true ;;
        --with-wiki)     with_wiki="target/xt-wiki" ;;
        --with-wiki-slice) with_wiki="target/xt-wiki-slice" ;;
        --with-wiki-dev) with_wiki="target/xt-wiki-dev" ;;
        --app)           with_app=true ;;
        --demo)          with_demo=true ;;
        *) echo "unknown option: $arg" >&2; exit 1 ;;
    esac
done

package="$repo_root/swift/Xtriever"
frameworks="$package/Frameworks"
generated="$package/Sources/Xtriever/Generated"
resources="$package/Sources/Xtriever/XtrieverData"
staging="$repo_root/build/swift"

"$repo_root/scripts/check-toolchain.sh"

echo "==> building staticlibs ($profile)"
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
    # shellcheck disable=SC2086  # profile_flag is intentionally word-split (it may be empty)
    cargo build -q -p xtriever-ffi $profile_flag --target "$target"
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
#                      Verified empirically — 001 report F-006.
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

echo "==> staging resources"
# The whole tree is rebuilt from this invocation's flags: a run without --with-models must not
# leave models staged by an earlier run for the tests to find. The directory itself must exist
# for `resources: [.copy("XtrieverData")]` to validate even when nothing is staged.
rm -rf "$resources"
mkdir -p "$resources"
printf 'Staged by scripts/build-ios-package.sh; gitignored.\n' > "$resources/README.txt"

if [ "$with_models" = true ]; then
    # Feature 026: the eight-bit artefacts by default (25 MB each against 90), each directory
    # staged whole — the GGUF beside the float model's config and tokenizer, and for the
    # re-ranker the borrowed pooler — as the manifests pin them. XTRIEVER_MODEL_MANIFEST and
    # XTRIEVER_RERANK_MODEL_MANIFEST name other manifests (e.g. the float ones); the engine
    # tells the artefacts apart by their weights file, so nothing else changes.
    embedder_manifest="${XTRIEVER_MODEL_MANIFEST:-$repo_root/reference/models/manifest-q8.json}"
    reranker_manifest="${XTRIEVER_RERANK_MODEL_MANIFEST:-$repo_root/reference/models/manifest-rerank-q8.json}"
    "$repo_root/scripts/fetch-model.sh" --manifest "$embedder_manifest" >/dev/null
    "$repo_root/scripts/fetch-model.sh" --manifest "$reranker_manifest" >/dev/null
    # `.local_dir` with the fetch script's own default, so a manifest without it (the float
    # embedder's had none until Feature 026) selects the directory the fetch script filled.
    embedder_dir="$repo_root/reference/models/$(jq -r '.local_dir // "all-MiniLM-L6-v2"' "$embedder_manifest")"
    reranker_dir="$repo_root/reference/models/$(jq -r '.local_dir // "ms-marco-MiniLM-L-6-v2"' "$reranker_manifest")"
    # Exactly the files the manifest pins — its own, the borrowed ones and any cut tensors —
    # never the directory as found: a float weights file left beside the GGUF, or an
    # interrupted download, would pass the fetch script's verification (it checks only the
    # pinned files) and be refused by the engine on the device, or bloat the bundle.
    stage_pinned() { # manifest source_dir dest_dir
        mkdir -p "$3"
        jq -r '[.files[].name] + (.borrows.files // []) + [.borrows.tensors.file // empty] | .[]' "$1" \
            | while IFS= read -r name; do cp "$2/$name" "$3/$name"; done
    }
    rm -rf "$resources/models"
    stage_pinned "$embedder_manifest" "$embedder_dir" "$resources/models/embedder"
    stage_pinned "$reranker_manifest" "$reranker_dir" "$resources/models/reranker"
    printf '    models bundled (%s): %s, %s\n' "$(du -sh "$resources/models" | cut -f1)" "$(basename "$embedder_dir")" "$(basename "$reranker_dir")"
else
    printf '    models NOT bundled — pass --with-models for the Swift tests and device runs\n'
fi

if [ "$with_fixtures" = true ]; then
    fixtures="$package/Tests/Fixtures"
    cargo run -q --release -p xtriever-ffi --example fixture_index -- "$fixtures"
    rm -rf "$resources/fixtures"; mkdir -p "$resources/fixtures"
    cp -R "$fixtures/index" "$resources/fixtures/index"
    cp "$fixtures/expected.json" "$resources/fixtures/expected.json"
    printf '    fixture index + goldens staged (%s)\n' "$(du -sh "$resources/fixtures" | cut -f1)"
fi

if [ "$with_scifact" = true ]; then
    scifact_index="$repo_root/target/xt-rerank-index/scifact"
    if [ ! -f "$scifact_index/xtriever-pipeline.json" ]; then
        printf 'build-ios-package: FAIL — no SciFact index at %s. Build it with:\n' "$scifact_index" >&2
        printf '  cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config hybrid-rerank-v1 --cache-dir target/xt-dense-cache --index-dir %s --out /tmp/scifact.json\n' "$scifact_index" >&2
        exit 1
    fi
    rm -rf "$resources/scifact"; mkdir -p "$resources/scifact"
    cp -R "$scifact_index" "$resources/scifact/index"
    # The first 20 judged queries by id (research D9), from the pinned dataset files.
    python3 - "$repo_root/reference/datasets/beir/scifact" "$resources/scifact/queries.json" <<'EOF'
import json, sys
root, out = sys.argv[1], sys.argv[2]
judged = set()
with open(f"{root}/qrels/test.tsv") as fh:
    next(fh)
    for line in fh:
        judged.add(line.split("\t")[0])
texts = {}
with open(f"{root}/queries.jsonl") as fh:
    for line in fh:
        q = json.loads(line)
        texts[q["_id"]] = q["text"]
ids = sorted((i for i in judged if i in texts), key=lambda s: (len(s), s))[:20]
if len(ids) != 20:
    # The protocol is 20 fixed queries; a shorter set would stage an incomplete measurement that
    # the device run would then carry out without saying so.
    sys.exit(f"build-ios-package: FAIL — {len(ids)} judged SciFact queries with text, need 20")
json.dump([{"id": i, "text": texts[i]} for i in ids], open(out, "w"), indent=2)
print(f"    {len(ids)} measurement queries at {out}")
EOF
    cargo run -q --release -p xtriever-ffi --example fixture_index -- --scifact \
        "$scifact_index" "$resources/scifact/queries.json" "$resources/scifact/expected-scifact.json"
    printf '    SciFact staged (%s)\n' "$(du -sh "$resources/scifact" | cut -f1)"
fi

if [ -n "$with_wiki" ]; then
    wiki="$repo_root/$with_wiki"
    if [ ! -f "$wiki/index/xtriever-pipeline.json" ]; then
        printf 'build-ios-package: FAIL — no Wikipedia index at %s. Build it with:\n' "$wiki" >&2
        case "$with_wiki" in
            target/xt-wiki-slice) limit=" --limit 3922" ;;
            target/xt-wiki-dev) limit=" --limit 2000" ;;
            *) limit="" ;;
        esac
        printf '  RAYON_NUM_THREADS=1 cargo run --release -p xtriever-cli -- wiki build --out %s --cache-dir target/xt-wiki-cache%s\n' \
            "$with_wiki" "$limit" >&2
        printf '  cargo run --release -p xtriever-cli -- wiki expected --index %s/index --out %s/expected.json\n' \
            "$with_wiki" "$with_wiki" >&2
        exit 1
    fi
    for f in wiki-build.json ATTRIBUTION.txt expected.json; do
        [ -f "$wiki/$f" ] || { printf 'build-ios-package: FAIL — %s/%s missing (run `xtriever wiki expected` for expected.json)\n' "$wiki" "$f" >&2; exit 1; }
    done
    if [ "$with_wiki" = target/xt-wiki-slice ]; then
        # The limit the index was built with, as the app reads it — not the README's number.
        slice_articles="$(plutil -extract partial raw -o - "$wiki/index/corpus.json" 2>/dev/null || true)"
        if [ -z "$slice_articles" ]; then
            printf 'build-ios-package: FAIL — %s/index/corpus.json has no article limit: not a --limit build\n' "$wiki" >&2
            exit 1
        fi
    fi
    rm -rf "$resources/wikipedia"; mkdir -p "$resources/wikipedia"
    cp -R "$wiki/index" "$resources/wikipedia/index"
    cp "$wiki/wiki-build.json" "$wiki/ATTRIBUTION.txt" "$wiki/expected.json" "$resources/wikipedia/"
    cp "$repo_root/reference/fixtures/008/queries.json" "$resources/wikipedia/queries.json"
    if [ "$with_wiki" = target/xt-wiki-dev ]; then
        printf '    Wikipedia DEV index staged (%s) — a --limit build; not the artefact\n' "$(du -sh "$resources/wikipedia" | cut -f1)"
    elif [ "$with_wiki" = target/xt-wiki-slice ]; then
        printf '    Wikipedia SLICE index staged (%s) — the first %s articles; not the whole edition\n' \
            "$(du -sh "$resources/wikipedia" | cut -f1)" "$slice_articles"
    else
        printf '    Wikipedia index staged (%s)\n' "$(du -sh "$resources/wikipedia" | cut -f1)"
    fi
    # The bundle budget (contracts/artefact.md "Staging"): 2,000,000,000 bytes for everything
    # staged. A miss is a build failure with the number, never a quiet oversize app.
    budget=2000000000
    # Logical file sizes (what the bundle carries), not allocated blocks: APFS compression,
    # clones and block rounding make `du` the wrong instrument for a byte contract.
    staged_bytes="$(find "$resources" -type f -exec stat -f%z {} + | awk '{s += $1} END {print s + 0}')"
    if [ "$staged_bytes" -gt "$budget" ]; then
        printf 'build-ios-package: FAIL — staged resources are %s bytes, over the %s-byte bundle budget\n' "$staged_bytes" "$budget" >&2
        exit 1
    fi
    printf '    staged resources %s bytes of the %s-byte budget\n' "$staged_bytes" "$budget"
fi

if [ "$with_app" = true ]; then
    # A physical device cannot run a hostless XCTest bundle ("Tool-hosted testing is unavailable
    # on device destinations" — 001 F-007), so the tests need an application to host them. The
    # .xcodeproj is derived, not authored — regenerated here and gitignored.
    if command -v xcodegen >/dev/null 2>&1; then
        (cd "$repo_root/swift/XtrieverHarnessApp" && xcodegen generate >/dev/null)
        printf '    XtrieverHarnessApp.xcodeproj regenerated\n'
    else
        # Reporting PASS here would let a caller believe the harness is ready and only discover
        # the missing host project during the device run itself.
        printf 'build-ios-package: INCOMPLETE — xcodegen is not installed, so the host app project\n' >&2
        printf '  was not generated. The SIMULATOR path works; a DEVICE run cannot. Install it with\n' >&2
        printf '  `brew install xcodegen` and re-run with --app.\n' >&2
        exit 2
    fi
fi

if [ "$with_demo" = true ]; then
    if command -v xcodegen >/dev/null 2>&1; then
        (cd "$repo_root/apps/ios-wiki-demo" && xcodegen generate >/dev/null)
        printf '    XtrieverWikiDemo.xcodeproj regenerated\n'
    else
        printf 'build-ios-package: INCOMPLETE — xcodegen is not installed, so the demo app project\n' >&2
        printf '  was not generated. Install it with `brew install xcodegen` and re-run with --demo.\n' >&2
        exit 2
    fi
fi

echo "build-ios-package: PASS"
printf '    xcframework %s  (%s)\n' "$frameworks/XtrieverFFI.xcframework" \
    "$(du -sh "$frameworks/XtrieverFFI.xcframework" | cut -f1)"
