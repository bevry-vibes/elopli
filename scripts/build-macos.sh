#!/bin/sh
# Build the Rust core, generate the UniFFI Swift bindings, and build the macOS app.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP_DIR="$ROOT/apps/macos/Elopli"
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target"}
CONFIG=${1:-debug}

cd "$ROOT"

if [ "$CONFIG" = "release" ]; then
	cargo build --release -p elopli-ffi
	LIB="$TARGET_DIR/release/libelopli_ffi.a"
	DYLIB="$TARGET_DIR/release/libelopli_ffi.dylib"
else
	cargo build -p elopli-ffi
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
		build
fi

echo "App: $APP_DIR/build/Build/Products/$([ "$CONFIG" = "release" ] && echo Release || echo Debug)/Elopli.app"
