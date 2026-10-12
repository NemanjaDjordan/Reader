# Contributing

Thank you for helping improve Reader.

## Before opening a pull request

1. Describe the problem and the intended behavior.
2. Keep platform-independent logic in Rust whenever practical.
3. Keep slow file, archive, image, and database work away from the UI thread.
4. Preserve original user files. Importing or removing a library item must not
   move or delete the source document.
5. Add or update regression tests for behavior changes.
6. Run:

   ```bash
   cargo fmt --check
   cargo test --locked
   ```

7. Build or test every platform affected by the change.

## Code and UI guidelines

- Prefer clear, small functions and explicit error handling.
- Keep comic image caches bounded and avoid loading an entire archive into RAM.
- Preserve Android SAF permissions and URI-based storage behavior.
- Keep mobile layouts usable on narrow screens without horizontal scrolling.
- User-facing application text is currently English.
- Do not commit copyrighted books, comics, private databases, signing keys, or
  generated build directories.

## Reporting bugs

Include the operating system, Reader version, document format, steps to
reproduce, and relevant terminal output. Do not attach copyrighted documents;
use a freely distributable minimal test file when possible.

## Licensing contributions

By submitting a contribution, you agree that your contribution may be
distributed under the project's MIT License. Third-party code and assets must
include their original license and attribution information.

