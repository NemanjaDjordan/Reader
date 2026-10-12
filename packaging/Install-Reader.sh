#!/usr/bin/env bash
set -euo pipefail

package_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
install_root="${XDG_DATA_HOME:-$HOME/.local/share}/reader"
bin_dir="$HOME/.local/bin"
applications_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"

mkdir -p "$install_root" "$bin_dir" "$applications_dir" "$icons_dir"
cp -a "$package_dir/." "$install_root/"
ln -sfn "$install_root/Pokreni-Citac.sh" "$bin_dir/reader"
install -m 644 "$package_dir/reader.png" "$icons_dir/reader.png"
install -m 644 "$package_dir/reader.desktop" "$applications_dir/reader.desktop"

echo "Reader was installed for the current user."
echo "Start it from the application menu or run: $bin_dir/reader"
