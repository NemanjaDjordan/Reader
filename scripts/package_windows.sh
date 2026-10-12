#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project_root"

target=x86_64-pc-windows-gnu
tool_prefix=x86_64-w64-mingw32
toolchain_dir=${LLVM_MINGW_DIR:-/tmp/llvm-mingw}
if [[ ! -x "$toolchain_dir/bin/$tool_prefix-clang" ]]; then
    echo "LLVM MinGW toolchain was not found at $toolchain_dir" >&2
    exit 1
fi

export PATH="$toolchain_dir/bin:$PATH"
export CC_x86_64_pc_windows_gnu="$tool_prefix-clang"
export CXX_x86_64_pc_windows_gnu="$tool_prefix-clang++"
export AR_x86_64_pc_windows_gnu="$tool_prefix-ar"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$tool_prefix-clang"
export CXXFLAGS_x86_64_pc_windows_gnu="-I$project_root/packaging/windows/include"

# Rust's GNU Windows target requests GCC compatibility library names, while
# llvm-mingw provides their LLVM/libc++ equivalents. Keep the aliases local to
# the downloaded toolchain and add them to the native linker search path.
compat_dir="$toolchain_dir/rust-gnu-compat"
mkdir -p "$compat_dir"
builtins=$(find "$toolchain_dir/lib/clang" \
    -path '*/lib/windows/libclang_rt.builtins-x86_64.a' -print -quit)
if [[ -z "$builtins" ]]; then
    echo "LLVM builtins library was not found in $toolchain_dir" >&2
    exit 1
fi
ln -sfn "$toolchain_dir/x86_64-w64-mingw32/lib/libc++.a" "$compat_dir/libstdc++.a"
ln -sfn "$toolchain_dir/x86_64-w64-mingw32/lib/libunwind.a" "$compat_dir/libgcc_eh.a"
ln -sfn "$builtins" "$compat_dir/libgcc.a"
export RUSTFLAGS="${RUSTFLAGS:-} -L native=$compat_dir"

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
package_name="Reader-${version}-windows-x86_64"
package_dir="$project_root/dist/$package_name"

if [[ -e "$package_dir" ]]; then
    echo "Package already exists: $package_dir" >&2
    exit 1
fi

cargo build --release --target "$target" --bin citac

mkdir -p "$package_dir"
install -m 755 "target/$target/release/citac.exe" "$package_dir/Reader.exe"
install -m 644 packaging/windows/Install-Reader.cmd "$package_dir/Install-Reader.cmd"
install -m 644 packaging/windows/Uninstall-Reader.cmd "$package_dir/Uninstall-Reader.cmd"
install -m 644 packaging/windows/README.txt "$package_dir/README.txt"

archive_path="$project_root/dist/$package_name.zip"
if command -v zip >/dev/null 2>&1; then
    (
        cd "$project_root/dist"
        zip -9 -r "$(basename "$archive_path")" "$package_name"
    )
else
    (
        cd "$project_root/dist"
        7z a -tzip -mx=9 "$(basename "$archive_path")" "$package_name" >/dev/null
    )
fi
sha256sum "$archive_path" > "$archive_path.sha256"
echo "Windows package: $archive_path"
