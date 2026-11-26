#!/bin/sh
# Build the Linux app and CLI, then create a portable tarball. When
# appimagetool is available, also create an AppImage.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if [ -z "$version" ]; then
	echo "cannot read the version from Cargo.toml" >&2
	exit 1
fi

case $(uname -m) in
	x86_64) arch=x86_64 ;;
	aarch64) arch=aarch64 ;;
	*) arch=$(uname -m) ;;
esac

target_dir=${CARGO_TARGET_DIR:-$root/target}
case $target_dir in
	/*) ;;
	*) target_dir="$root/$target_dir" ;;
esac

cargo build --locked --release -p elopli-app-linux --features linux
cargo build --locked --release -p elopli

app="$target_dir/release/elopli-app"
cli="$target_dir/release/elopli"
if [ ! -x "$app" ]; then
	echo "missing $app; the Linux app build did not produce a binary" >&2
	exit 1
fi
if [ ! -x "$cli" ]; then
	echo "missing $cli; the CLI build did not produce a binary" >&2
	exit 1
fi

dist="$root/dist"
package="Elopli-$version-linux-$arch"
staging="$dist/$package"
rm -rf "$staging"
mkdir -p "$staging/cli" \
	"$staging/share/applications" \
	"$staging/share/metainfo" \
	"$staging/share/icons"
install -m755 "$app" "$staging/elopli-app"
install -m755 "$cli" "$staging/cli/elopli"
install -m644 apps/linux/Elopli/assets/com.bevry.elopli.desktop \
	"$staging/share/applications/com.bevry.elopli.desktop"
install -m644 apps/linux/Elopli/assets/com.bevry.elopli.metainfo.xml \
	"$staging/share/metainfo/com.bevry.elopli.metainfo.xml"
cp -R apps/linux/Elopli/assets/icons/hicolor "$staging/share/icons/"

for file in README.md LICENSE.md PRIVACY.md SECURITY.md CHANGELOG.md \
	THIRD-PARTY-NOTICES.md devices.example.json; do
	install -m644 "$file" "$staging/$file"
done
mkdir -p "$staging/docs"
install -m644 docs/api.md docs/linux.md docs/platforms.md \
	"$staging/docs/"

desktop-file-validate "$staging/share/applications/com.bevry.elopli.desktop"
appstreamcli validate --no-net "$staging/share/metainfo/com.bevry.elopli.metainfo.xml"

(
	cd "$staging"
	find . -type f ! -name SHA256SUMS -print0 |
		sort -z |
		xargs -0 sha256sum |
		sed 's#  \./#  #'
) > "$staging/SHA256SUMS"

tarball="$dist/$package.tar.gz"
rm -f "$tarball"
tar -czf "$tarball" -C "$dist" "$package"

appimage_tool=${APPIMAGETOOL:-}
if [ -z "$appimage_tool" ] && command -v appimagetool >/dev/null 2>&1; then
	appimage_tool=appimagetool
fi
if [ -n "$appimage_tool" ]; then
	appdir="$dist/Elopli.AppDir"
	rm -rf "$appdir"
	mkdir -p "$appdir/usr/bin" \
		"$appdir/usr/share/applications" \
		"$appdir/usr/share/metainfo" \
		"$appdir/usr/share/icons"
	install -m755 "$app" "$appdir/usr/bin/elopli-app"
	install -m755 "$cli" "$appdir/usr/bin/elopli"
	install -m644 apps/linux/Elopli/assets/com.bevry.elopli.desktop \
		"$appdir/usr/share/applications/com.bevry.elopli.desktop"
	install -m644 apps/linux/Elopli/assets/com.bevry.elopli.metainfo.xml \
		"$appdir/usr/share/metainfo/com.bevry.elopli.metainfo.xml"
	cp -R apps/linux/Elopli/assets/icons/hicolor "$appdir/usr/share/icons/"
	install -m644 apps/linux/Elopli/assets/com.bevry.elopli.desktop \
		"$appdir/com.bevry.elopli.desktop"
	install -m644 apps/linux/Elopli/assets/icons/hicolor/256x256/apps/com.bevry.elopli.png \
		"$appdir/com.bevry.elopli.png"
	install -m644 apps/linux/Elopli/assets/icons/hicolor/256x256/apps/com.bevry.elopli.png \
		"$appdir/.DirIcon"
	cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$here/usr/bin/elopli-app" "$@"
APPRUN
	chmod 755 "$appdir/AppRun"
	# The Rust appimagetool takes -o/-n; the classic tool takes APPDIR OUTPUT.
	if "$appimage_tool" --help 2>&1 | grep -q -- '--output'; then
		"$appimage_tool" --appimage-arch "$arch" --arch "$arch" \
			--output "$dist" --name "$package.AppImage" "$appdir"
	else
		ARCH="$arch" "$appimage_tool" "$appdir" "$dist/$package.AppImage"
	fi
else
	echo "appimagetool was not found; wrote the portable tarball only" >&2
fi

echo "Wrote $staging"
echo "Wrote $tarball"
if [ -f "$dist/$package.AppImage" ]; then
	echo "Wrote $dist/$package.AppImage"
fi
