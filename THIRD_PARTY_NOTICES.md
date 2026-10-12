# Third-party notices

The MIT License in the repository root applies to the original Reader project
code. Dependencies and bundled third-party components retain their own terms.
This document is a practical summary, not a replacement for their licenses.

## Slint

Reader uses Slint 1.18. Slint is offered under multiple licenses, including
GPL-3.0-only and the Slint Royalty-free Desktop, Mobile, and Web Applications
License 2.0. Developers choosing the royalty-free application license must
follow its attribution condition, for example by displaying the Slint
attribution badge on the application's public download webpage or by providing
the documented AboutSlint UI.

- <https://slint.dev>
- <https://github.com/slint-ui/slint>

Review the exact license files shipped with the selected Slint version before
distributing binaries.

## Qt

The optional Linux EPUB helper dynamically links to Qt 6 Core and Gui. Qt
components used by distributed packages are commonly available under LGPL
terms. Distributors must preserve the applicable notices, provide the required
license text, and allow replacement/relinking of the dynamically linked Qt
libraries as required by the selected Qt license.

- <https://www.qt.io/licensing/open-source-lgpl-obligations>

## UnRAR

CBR support uses UnRAR source through the vendored `unrar_sys` crate. The full
UnRAR license is stored at:

`vendor/unrar_sys/vendor/unrar/license.txt`

Important restriction: the UnRAR source may be used to handle RAR archives,
but it may not be used to develop a RAR-compatible archiver or recreate the
proprietary RAR compression algorithm. Modified source may be redistributed
only with the required UnRAR license paragraph and notices.

## Material Design Icons

Interface icons in `assets/icons/` are adapted from Google Material Design
Icons and are licensed under Apache License 2.0. The notice and full license
are included in:

- `assets/icons/LICENSE.md`
- `assets/icons/LICENSE-APACHE-2.0.txt`

## Rust crates

Rust dependencies are listed and version-locked in `Cargo.toml` and
`Cargo.lock`. Each crate retains its own license. Binary distributors should
generate and review a complete dependency license report, for example with
`cargo-about`, before each public release.

## Application icon

The Reader application icon was supplied for this project and is distributed
as part of the original project under the repository's MIT License. A
maintainer publishing this repository should confirm that they hold the right
to license the source image under those terms.

