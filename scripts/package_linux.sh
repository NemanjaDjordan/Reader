#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project_root"

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
package_name="Citac-${version}-linux-x86_64"
package_dir="$project_root/dist/$package_name"

if [[ -e "$package_dir" ]]; then
    echo "Paket već postoji: $package_dir" >&2
    echo "Premjestite ga ili obrišite prije ponovnog pakovanja." >&2
    exit 1
fi

cargo build --release

helper=$(find target/release/build -path '*/out/qt-epub-renderer/citac-epub-qt-prototype' -type f -print -quit)
if [[ -z "$helper" ]]; then
    echo "Qt EPUB pomoćni renderer nije pronađen u release buildu." >&2
    exit 1
fi

qt_plugins=$(qtpaths6 --query QT_INSTALL_PLUGINS)
mkdir -p "$package_dir/lib" "$package_dir/plugins/platforms" \
    "$package_dir/plugins/imageformats" "$package_dir/licenses"

install -m 755 target/release/citac "$package_dir/citac"
install -m 755 "$helper" "$package_dir/citac-epub-qt-prototype"
install -m 755 "$qt_plugins/platforms/libqoffscreen.so" \
    "$package_dir/plugins/platforms/libqoffscreen.so"

for plugin_name in libqgif.so libqjpeg.so libqsvg.so libqwebp.so; do
    if [[ -f "$qt_plugins/imageformats/$plugin_name" ]]; then
        install -m 755 "$qt_plugins/imageformats/$plugin_name" \
            "$package_dir/plugins/imageformats/$plugin_name"
    fi
done

copy_qt_libraries() {
    local binary=$1
    while read -r library; do
        [[ -n "$library" && -f "$library" ]] || continue
        cp -L "$library" "$package_dir/lib/$(basename "$library")"
        chmod 755 "$package_dir/lib/$(basename "$library")"
    done < <(ldd "$binary" | awk '/libQt6[^ ]*\.so/ { print $3 }' | sort -u)
}

copy_qt_libraries "$package_dir/citac-epub-qt-prototype"
copy_qt_libraries "$package_dir/plugins/platforms/libqoffscreen.so"
for plugin in "$package_dir"/plugins/imageformats/*.so; do
    [[ -e "$plugin" ]] || continue
    copy_qt_libraries "$plugin"
done

qt_license_dir=/usr/share/licenses/qt6-base
if [[ -d "$qt_license_dir" ]]; then
    cp -a "$qt_license_dir" "$package_dir/licenses/qt6-base"
fi

unrar_license=$(find "$HOME/.cargo/registry/src" -path '*/unrar_sys-0.5.8/vendor/unrar/license.txt' -type f -print -quit 2>/dev/null || true)
if [[ -n "$unrar_license" ]]; then
    install -m 644 "$unrar_license" "$package_dir/licenses/UNRAR-LICENSE.txt"
fi

install -m 644 assets/icons/LICENSE.md "$package_dir/licenses/MATERIAL-ICONS-NOTICE.md"
install -m 644 assets/icons/LICENSE-APACHE-2.0.txt \
    "$package_dir/licenses/APACHE-2.0.txt"

install -m 644 packaging/README-Linux.txt "$package_dir/README.txt"
install -m 755 packaging/Pokreni-Citac.sh "$package_dir/Pokreni-Citac.sh"
install -m 755 packaging/Install-Reader.sh "$package_dir/Install-Reader.sh"
install -m 644 packaging/reader.desktop "$package_dir/reader.desktop"
install -m 644 assets/app-icon/app-icon-256.png "$package_dir/reader.png"

echo "Napravljen paket: $package_dir"
du -sh "$package_dir"

archive_path="$project_root/dist/$package_name.tar.gz"
tar -C "$project_root/dist" -czf "$archive_path" "$package_name"
sha256sum "$archive_path" > "$archive_path.sha256"
echo "Arhiva: $archive_path"
