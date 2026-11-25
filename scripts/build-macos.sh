#!/bin/sh
# Build the Rust core, generate the UniFFI Swift bindings, and build the macOS app.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP_DIR="$ROOT/apps/macos/Optoli"
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target"}
CONFIG=${1:-debug}

cd "$ROOT"

if [ "$CONFIG" = "release" ]; then
	cargo build --release -p optoli-ffi
	LIB="$TARGET_DIR/release/liboptoli_ffi.a"
	DYLIB="$TARGET_DIR/release/liboptoli_ffi.dylib"
else
	cargo build -p optoli-ffi
	LIB="$TARGET_DIR/debug/liboptoli_ffi.a"
	DYLIB="$TARGET_DIR/debug/liboptoli_ffi.dylib"
fi

rm -rf "$APP_DIR/Generated"
mkdir -p "$APP_DIR/Generated/FFI"
cargo run -q -p uniffi-bindgen -- generate \
	--library "$DYLIB" \
	--language swift \
	--out-dir "$APP_DIR/Generated"

cp "$APP_DIR/Generated/optoli_ffiFFI.h" "$APP_DIR/Generated/FFI/"
cp "$APP_DIR/Generated/optoli_ffiFFI.modulemap" "$APP_DIR/Generated/FFI/module.modulemap"
cp "$LIB" "$APP_DIR/Generated/FFI/"

cd "$APP_DIR"
xcodegen generate
if [ -n "${DEVELOPMENT_TEAM:-}" ]; then
	xcodebuild \
		-project Optoli.xcodeproj \
		-scheme Optoli \
		-configuration "$([ "$CONFIG" = "release" ] && echo Release || echo Debug)" \
		-derivedDataPath build \
		CODE_SIGN_STYLE=Automatic \
		DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" \
		build
elif [ -n "${SIGN_IDENTITY:-}" ]; then
	xcodebuild \
		-project Optoli.xcodeproj \
		-scheme Optoli \
		-configuration "$([ "$CONFIG" = "release" ] && echo Release || echo Debug)" \
		-derivedDataPath build \
		CODE_SIGN_STYLE=Manual \
		CODE_SIGN_IDENTITY="$SIGN_IDENTITY" \
		DEVELOPMENT_TEAM="" \
		build
else
	xcodebuild \
		-project Optoli.xcodeproj \
		-scheme Optoli \
		-configuration "$([ "$CONFIG" = "release" ] && echo Release || echo Debug)" \
		-derivedDataPath build \
		CODE_SIGN_STYLE=Manual \
		CODE_SIGN_IDENTITY="-" \
		DEVELOPMENT_TEAM="" \
		build
fi

echo "App: $APP_DIR/build/Build/Products/$([ "$CONFIG" = "release" ] && echo Release || echo Debug)/Optoli.app"
