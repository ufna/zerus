# Desktop translations

`zerus_ru.ts` is the editable Russian Qt Linguist catalog. Update source locations
and extract new messages from the repository root with:

```sh
lupdate tray/src -no-obsolete -ts tray/translations/zerus_ru.ts
```

The Qt tools may be in the installation's `lib/qt6/bin` directory rather than
`PATH`. Complete new entries in Qt Linguist before building. Preserve numbered
placeholders, `%n`, HTML markup and the three Russian plural forms.

CMake validates the catalog with `prepare_catalog.py`, adds a shared context for
widgets that inherit `tr()` without `Q_OBJECT`, compiles it with `lrelease` and
embeds the result. The shared context is generated in the build directory; do
not edit or commit the generated `.ts`, `.qm` or resource files. Ambiguous
translations retain their original Qt contexts without a shared fallback.

The language preference is stored locally in `workspace/language`. English,
Russian and the system language are available in Settings → Appearance.
Restart the desktop to apply a change. Agent processes and native conversation
content are independent of the desktop language.

Catalog validation and the isolated UI language checks run through CTest.
The validator tests can also run directly:

```sh
python3 tray/translations/test_prepare_catalog.py
```
