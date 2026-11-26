#!/bin/sh
# Build the Rust core, generate the UniFFI Swift bindings, and build the macOS app.
set -eu

ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
APP_DIR="$ROOT/apps/macos/Elopli"
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target"}
CONFIG=${1:-debug}
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
export CODE_SIGNING_ALLOWED="${CODE_SIGNING_ALLOWED:-YES}"

cd "$ROOT"

# Cargo.toml is the single version source. The build number is the commit
# count, so macOS and Windows packages can share it.
MARKETING_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)
CURRENT_PROJECT_VERSION=$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)
export MARKETING_VERSION CURRENT_PROJECT_VERSION

if [ "$CONFIG" = "release" ]; then
	cargo build --locked --release -p elopli-ffi
	LIB="$TARGET_DIR/release/libelopli_ffi.a"
	DYLIB="$TARGET_DIR/release/libelopli_ffi.dylib"
else
	cargo build --locked -p elopli-ffi
	LIB="$TARGET_DIR/debug/libelopli_ffi.a"
	DYLIB="$TARGET_DIR/debug/libelopli_ffi.dylib"
fi

rm -rf "$APP_DIR/Generated"
mkdir -p "$APP_DIR/Generated/FFI"
cargo run -q -p uniffi-bindgen -- generate \
	--library "$DYLIB" \
	--language swift \
	--out-dir "$APP_DIR/Generated"

cp "$APP_DIR/Generated/elopli_ffiFFI.h" "$APP_DIR/Generated/FFI/"
cp "$APP_DIR/Generated/elopli_ffiFFI.modulemap" "$APP_DIR/Generated/FFI/module.modulemap"
cp "$LIB" "$APP_DIR/Generated/FFI/"

cd "$APP_DIR"
xcodegen generate
if [ -n "${DEVELOPMENT_TEAM:-}" ]; then
	xcodebuild \
		-project Elopli.xcodeproj \
		-scheme Elopli \
		-configuration "$([ "$CONFIG" = "release" ] && echo Release || echo Debug)" \
		-derivedDataPath build \
		CODE_SIGN_STYLE=Automatic \
		CODE_SIGN_IDENTITY="Apple Development" \
		DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" \
		CODE_SIGNING_ALLOWED="$CODE_SIGNING_ALLOWED" \
		-allowProvisioningUpdates \
		build
elif [ -n "${SIGN_IDENTITY:-}" ]; then
	xcodebuild \
		-project Elopli.xcodeproj \
		-scheme Elopli \
		-configuration "$([ "$CONFIG" = "release" ] && echo Release || echo Debug)" \
		-derivedDataPath build \
		CODE_SIGN_STYLE=Manual \
		CODE_SIGN_IDENTITY="$SIGN_IDENTITY" \
		DEVELOPMENT_TEAM="" \
		CODE_SIGNING_ALLOWED="$CODE_SIGNING_ALLOWED" \
		build
else
	xcodebuild \
		-project Elopli.xcodeproj \
		-scheme Elopli \
		-configuration "$([ "$CONFIG" = "release" ] && echo Release || echo Debug)" \
		-derivedDataPath build \
		CODE_SIGN_STYLE=Manual \
		CODE_SIGN_IDENTITY="-" \
		DEVELOPMENT_TEAM="" \
		CODE_SIGNING_ALLOWED="$CODE_SIGNING_ALLOWED" \
		build
fi

echo "App: $APP_DIR/build/Build/Products/$([ "$CONFIG" = "release" ] && echo Release || echo Debug)/Elopli.app"
