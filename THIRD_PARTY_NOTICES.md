# Third-party components

Zerus original source and artwork are licensed under the MIT license in `LICENSE`.
Dependency licenses and copyright notices remain in effect.

## Vendored code

`tray/vendor/libvterm` contains libvterm 0.3.3 under MIT. Its original copyright
notice and complete license are retained in `tray/vendor/libvterm/LICENSE`;
source provenance and archive checksum are in `UPSTREAM.md` beside it.

## Dependencies obtained during build

Rust dependencies and versions are locked in `Cargo.lock`. Their license
expressions and source repositories are recorded in `docs/rust-dependencies.json`.
This inventory is metadata, not a replacement for dependency license texts.
Cargo downloads the corresponding sources and license files from crates.io.

The GUI links Qt 6 (Widgets, Network and WebEngineWidgets), and on Linux the KDE
Frameworks KStatusNotifierItem and KWindowSystem. These retain their own licenses.
Qt WebEngine includes Chromium and additional third-party components. See the
[Qt WebEngine licensing documentation](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
and the notices supplied by the exact Qt distribution used to build the application.

This repository does not distribute Qt/KDE binaries or a bundled public installer.
Before publishing binary packages, collect license texts, notices and any required
source/relinking materials for the exact dependency versions shipped. MIT licensing
of Zerus itself does not relicense these dependencies.

Native agent CLIs are installed separately by the user and are not included here.
