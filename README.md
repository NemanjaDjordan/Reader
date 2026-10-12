# Reader

Reader is a native, cross-platform application for organizing and reading
books and comics. It is written in Rust, uses Slint for the user interface,
and targets Linux, Windows, and Android.

The project is open source and welcomes bug reports, improvements, ports,
translations, and new features.

## Features

- Local library with folders, covers, favorites, search, and reading progress
- EPUB, TXT, CBZ, and CBR support
- EPUB text, images, inline SVG, contents, search, and bookmarks
- Comic single-page and two-page layouts
- Fit-page, fit-width, and fit-height modes
- Touch gestures and responsive Android interface
- Android Storage Access Framework integration without permanently copying
  imported documents into the application's private storage
- SQLite library database and persistent reader state
- Native packages for Linux, Windows x86-64, and Android ARM64

PDF is not currently supported.

## Technology

- Rust 2024 edition
- Slint 1.18
- SQLite through `rusqlite`
- Qt 6 helper renderer for paginated EPUB display on Linux
- ZIP/CBZ and RAR/CBR archive readers

## Repository layout

- `src/` — application logic, library database, readers, and thumbnails
- `ui/` — Slint user interface
- `android/` — Android manifest, resources, and SAF integration
- `assets/` — application and interface icons
- `packaging/` — Linux and Windows packaging resources
- `scripts/` — build and packaging scripts
- `prototype/epub_qt/` — Qt EPUB helper used by the Linux build
- `vendor/unrar_sys/` — locally patched UnRAR binding used for cross-compilation
- `docs/` — original requirements and current project status

Generated files, build caches, user databases, private keys, and release
binaries are intentionally excluded from the source repository.

## Build requirements

Install a recent stable Rust toolchain with Cargo.

### Linux

The full Linux EPUB renderer requires:

- CMake 3.21 or newer
- a C++20 compiler
- Qt 6.5 or newer development packages with Core and Gui

Build and test:

```bash
cargo test
cargo run --release
```

Create the portable Linux package:

```bash
./scripts/package_linux.sh
```

The generated package is placed in `dist/`.

### Android

The current Android build targets ARM64, API 35, and requires an Android SDK,
Android NDK, Java tools, and `cargo-apk`.

```bash
rustup target add aarch64-linux-android
cargo install cargo-apk

export ANDROID_HOME=/path/to/android-sdk
export ANDROID_NDK_HOME=/path/to/android-sdk/ndk/<version>
./scripts/build_android.sh --release
```

The script currently expects Android platform 35 and build-tools 35.0.1.
For public releases, replace the generated development keystore with a secure
release signing configuration. Never commit a signing key.

### Windows from Linux

The Windows packaging script uses the Rust GNU target and an LLVM-MinGW
toolchain:

```bash
rustup target add x86_64-pc-windows-gnu
LLVM_MINGW_DIR=/path/to/llvm-mingw ./scripts/package_windows.sh
```

It creates a portable Windows package with per-user install and uninstall
scripts. A Windows code-signing certificate is not included.

## Tests

Run the complete regression suite with:

```bash
cargo fmt --check
cargo test --locked
```

## Current limitations

- Library titles do not yet use consistent natural sorting in every view.
- The Qt EPUB helper is currently integrated only into the Linux build.
- Complex reflowable EPUB CSS cannot always match a full browser engine.
- Windows packages can be cross-compiled on Linux, but should also be tested
  on a real Windows installation before publishing a release.
- Android release builds need the maintainer's own secure signing key.

See [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md) for the detailed project
history and remaining work.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before
submitting a pull request. Please include tests for behavior changes and verify
formatting, tests, and the relevant platform build.

## License

Original project code is available under the permissive MIT License. See
[LICENSE](LICENSE).

Third-party components retain their own licenses and conditions. In particular,
Slint attribution requirements, Qt LGPL terms, Material Icons attribution, and
the UnRAR restriction must still be followed. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Download the latest version for your operating system:
https://github.com/NemanjaDjordan/Reader/releases/latest

Available packages:

- Android ARM64 APK
- Linux x86-64
- Windows x86-64
