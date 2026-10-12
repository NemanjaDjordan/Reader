#!/usr/bin/env bash
set -euo pipefail

package_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export LD_LIBRARY_PATH="$package_dir/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export QT_PLUGIN_PATH="$package_dir/plugins"
export QT_QPA_PLATFORM_PLUGIN_PATH="$package_dir/plugins/platforms"
export CITAC_QT_EPUB_HELPER="$package_dir/citac-epub-qt-prototype"

exec "$package_dir/citac" "$@"
