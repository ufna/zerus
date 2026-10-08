# Third-party components

Zerus original source and artwork are licensed under the MIT license in `LICENSE`.
Dependency licenses and copyright notices remain in effect.

## Vendored code

`tray/vendor/libvterm` contains libvterm 0.3.3 under MIT. Its original copyright
notice and complete license are retained in `tray/vendor/libvterm/LICENSE`;
source provenance and archive checksum are in `UPSTREAM.md` beside it.

`tray/vendor/md4c` contains the md4c 0.5.2 Markdown parser and entity table
under MIT. Its original license is retained in `tray/vendor/md4c/LICENSE.md`;
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

The Arch package links Qt/KDE dynamically and requires their system packages; it
does not bundle Qt, KDE or Chromium. Their distribution supplies their notices
and corresponding source. `scripts/collect-licenses.py` collects the exact
host-target Cargo dependency license texts and inventory into the package,
alongside the Zerus and vendored libvterm and md4c licenses. A missing dependency notice
fails packaging. Review notices and any source/relinking obligations again if
the linkage or bundled components change. MIT licensing of Zerus itself does
not relicense its dependencies.

The CLI's `rusqlite` build includes SQLite through `libsqlite3-sys`. The collected
notices also preserve its upstream public-domain dedication, separate from the
Rust binding's MIT notice. [SQLite copyright](https://www.sqlite.org/copyright.html).

Native agent CLIs are installed separately by the user and are not included here.
