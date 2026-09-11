#!/usr/bin/env bash
# Assert that the Rust toolchain in this shell is the one `rust-toolchain.toml` pins.
#
# Why this exists: on this machine `/opt/homebrew/bin/cargo` precedes `~/.cargo/bin/cargo`
# on PATH. The Homebrew toolchain ignores `rust-toolchain.toml` and ships only the host
# `std`, so `cargo check --target aarch64-apple-ios` fails with
#
#     error[E0463]: can't find crate for `std`
#
# which looks exactly like the iOS portability failure this spike exists to detect.
# Any cross-target verdict recorded without this check is void.
#
# See specs/001-ios-build-spike/research.md D14.

set -euo pipefail

fail() {
    printf 'check-toolchain: FAIL — %s\n' "$1" >&2
    printf '  fix: %s\n' "$2" >&2
    exit 1
}

# 1. cargo must be rustup's shim, not Homebrew's binary.
cargo_path="$(command -v cargo || true)"
[ -n "$cargo_path" ] || fail "no cargo on PATH" 'install rustup, then export PATH="$HOME/.cargo/bin:$PATH"'
case "$cargo_path" in
    "$HOME"/.cargo/bin/*) ;;
    *) fail "cargo resolves to $cargo_path, not \$HOME/.cargo/bin/cargo" \
            'export PATH="$HOME/.cargo/bin:$PATH"' ;;
esac

# 2 & 3. Neither cargo nor rustc may be the Homebrew build. They can diverge
#        independently, so both are checked.
cargo_version="$(cargo --version)"
case "$cargo_version" in
    *Homebrew*) fail "cargo is the Homebrew build ($cargo_version)" \
                     'export PATH="$HOME/.cargo/bin:$PATH"' ;;
esac
rustc_version="$(rustc --version)"
case "$rustc_version" in
    *Homebrew*) fail "rustc is the Homebrew build ($rustc_version)" \
                     'export PATH="$HOME/.cargo/bin:$PATH"' ;;
esac

# 4. rust-toolchain.toml must actually be in effect.
pinned="$(awk -F'"' '/^channel/ {print $2}' "$(dirname "$0")/../rust-toolchain.toml")"
active="$(rustup show active-toolchain 2>/dev/null || true)"
case "$active" in
    *"$pinned"*) ;;
    *) fail "active toolchain is '${active:-unknown}', expected the pinned $pinned" \
            "rustup toolchain install $pinned" ;;
esac

# 5. Both iOS targets must be installed for the pinned toolchain.
installed="$(rustup target list --installed)"
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
    case "$installed" in
        *"$target"*) ;;
        *) fail "target $target is not installed" "rustup target add $target" ;;
    esac
done

# 6. Linking for the device needs a full Xcode, not just Command Line Tools.
#    `cargo check` passes without it; `cargo build --target aarch64-apple-ios` does not.
#    Catch that here rather than after eight verdicts have been recorded.
sdk_path="$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null || true)"
[ -n "$sdk_path" ] || fail "no iphoneos SDK (Command Line Tools only?)" \
    'install Xcode, then: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer'

printf 'check-toolchain: PASS — %s | %s | active=%s | targets=aarch64-apple-ios,aarch64-apple-ios-sim | sdk=%s\n' \
    "$cargo_version" "$rustc_version" "$pinned" "$(basename "$sdk_path")"
