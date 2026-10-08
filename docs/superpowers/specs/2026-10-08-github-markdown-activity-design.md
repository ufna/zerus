# GitHub-style Markdown in the Activity view

Status: approved design, 2026-10-08. Tracking: `zerus-0bg`.

## Goal

Agent replies in the Activity view render Markdown so that it is visually
indistinguishable from a GitHub comment body (`github-markdown-css` 5.8.1), in
both light and dark themes, while the view stays a `QTextBrowser`. Zerus cards,
headers, tool groups, attachments and user messages keep their current design.

A throwaway prototype (outside the repository) reached a pixel-level match with
the reference rendered by Chromium: block positions within 1 px, identical inline
code geometry. The techniques below are the ones that prototype validated.

## Non-goals

- Syntax highlighting. Code blocks are monochrome, as GitHub shows fences without
  a known language. Highlighting can follow as a separate task.
- Horizontal scrolling of wide code blocks and tables. Qt cannot scroll a single
  block; long code lines keep wrapping (current behaviour) and tables shrink.
- Markdown in user messages, tool details or question replies.
- Moving the Activity view to Qt WebEngine.

## Why the current output looks wrong

`ActivityView::markdown()` calls `QTextDocument::setMarkdown()` and exports
`toHtml()`. That HTML carries Qt's own block margins and list indents (40 px per
level), so the journal stylesheet barely applies. CSS for Markdown documents is an
open Qt request (QTBUG-75662). The fix is to generate HTML ourselves, restricted
to the [rich-text subset](https://doc.qt.io/qt-6/richtext-html-subset.html) that
Qt renders faithfully.

## Architecture

### `MarkdownHtml` (new, `tray/src/MarkdownHtml.{h,cpp}`)

A pure function from Markdown to Qt rich-text HTML:

```cpp
struct MarkdownTheme;            // colours and metrics, see below
struct MarkdownLinks {           // link policy owned by ActivityView
    std::function<QString(const QString &href, QString *suffix)> resolve;
};
QString MarkdownHtml::render(const QString &markdown, const MarkdownTheme &, const MarkdownLinks &);
```

- Parser: md4c 0.5.2 (the version Qt 6.11 bundles internally), vendored in
  `tray/vendor/md4c` with `LICENSE` and `UPSTREAM.md` (source URL and SHA-256),
  built by `tray/cmake/Markdown.cmake` like `Terminal.cmake` builds libvterm.
  The renderer uses md4c's callback API (`md_parse`), not `md4c-html`.
- Flags: `MD_DIALECT_GITHUB | MD_FLAG_NOHTML`. Raw HTML stays literal text, as
  today. `MD_FLAG_UNDERLINE` is deliberately off: GitHub renders `_x_` as
  emphasis, while Qt's dialect turned it into underline.
- No widget or document access; it is unit-testable on strings.

### Integration in `ActivityView`

- `markdown()` becomes a thin wrapper that builds `MarkdownTheme` and the link
  policy (existing rules, unchanged: `SessionFileReference` links become
  `hgs-file:<sha256>` with the visible location suffix; only `http`/`https` with a
  host and no user info stay clickable; other links become plain text; images
  become “[Image attachment]”).
- Agent message cards keep their Zerus background (`#f3f6f8` / `#242d36`); the
  Markdown inside uses GitHub typography and colours on it. Because GitHub's
  `canvas.subtle` would vanish on that grey, code blocks, zebra rows and code-block
  corners use the journal's code surface (`#e8eef2` / `#171e25`). Table stripes
  have their own token: `#e8eef2` in light, `#2a333d` (a step above the card) in
  dark, where borders use `#3a444e` and inline code is filled at 33 %. Expanded
  “Thinking” text uses the same renderer on the page background.
- `ContentScale::html()` keeps scaling every `px` length, so the renderer emits
  lengths in `px` at scale 1. Image resources are generated for the current scale.

## Rendering rules

Metrics are GitHub comment values: base font 14 px, `rem` = 16 px, scaled by the
Activity content scale. Spacing between blocks uses `margin-top` (16 px; headings
24 px) and the first block in a container has none: Qt ignores a paragraph's
`margin-bottom` before a table.

| Markdown | Qt HTML |
| --- | --- |
| Paragraph | `<p>` with fixed `line-height` 21 px, or 23 px when the block contains inline code (matches GitHub's taller line box). |
| Heading 1–6 | Single-cell table; text at 2/1.5/1.25/1/0.875/0.85 em, weight 600; h1/h2 have a 1 px bottom border (`border.muted`) and 0.3 em padding; h6 uses `fg.muted`. |
| Lists | One table per list, one cell per item with a 28 px left padding. The marker hangs on the item's first line (`text-indent: -28px`) as a 28 px wide image, so it shares the text baseline whatever font the line uses (a separate marker cell drifted under fixed line heights). Items after the first get 3 px top padding (16 px in loose lists). Nested lists sit in the cell without extra margin. Markers: disc → circle → square for bullets, the dot 5.5 px above the baseline; decimal → lower-roman → lower-alpha for ordered lists, right-aligned text between transparent spacers, honouring the start number. |
| Task list item | The hanging marker is a drawn checkbox (checked/unchecked) centred on the line, no bullet. |
| Inline code | A text object (see below): 85 % of the surrounding text, with its colour, weight and slant; inside headings the heading size and 0 .2em padding (GitHub's `h1 code { font-size: inherit }`). |
| Code block | 3×3 table: 6 px corner images with a 6 px radius, `canvas.subtle` edges, centre cell padded 7/10/13/10 px (top/right/bottom/left) with the monospace font at 12 px and 17 px line height. Text wraps. |
| Blockquote | Two-cell table: 4 px left bar (`border.default`) and a content cell with 14 px horizontal padding in `fg.muted`. |
| Table | `border-collapse` table, cells padded 6 px 13 px with 1 px `border.default`; header cells bold and centred unless aligned; even rows `canvas.subtle`. Column alignment from Markdown. |
| Thematic break | Full-width 4 px bar in `border.default`, 24 px above and below. |
| Emphasis, strong, strikethrough | `<i>`, `<b>` (600), `<s>`. |
| Link | `fg.accent`, no underline, href from the link policy. |
| Hard break | `<br>`. |

### Inline code as text objects

Qt fills a text background across the whole line box, so a styled span can never
match GitHub's chip (padding 0.2 em 0.4 em, radius 6 px). Inline code is therefore
emitted as a span with a sentinel background colour and, right after `setHtml()`,
each such run is replaced by one `QTextObjectInterface` object that paints the
rounded rectangle and the code text itself (monospace, 85 % of the base size).
The object uses `AlignMiddle`; its format font is chosen so that Qt's centring
(`xHeight / 4` above the baseline) places the chip where GitHub does.

Conversion happens before `ActivityView` restores bookmarks and the selection, so
offsets are computed in converted documents on both sides of a refresh.

Trade-offs, accepted:

- A chip is selected as a whole, like an image; partial selection inside it is not
  possible.
- In search-result mode chips are not converted (they stay a styled span with the
  chip colour), so `QTextDocument::find()` highlights matches inside code.
- A run longer than 40 characters, or a chip wider than half the Activity pane,
  also stays styled text: a chip cannot wrap, and a long path or command must not
  be clipped in a narrow pane or at a large content scale. The view re-renders
  chips (debounced) when the pane width changes by more than 10 %.

### Drawn resources

Markers, checkboxes and code-block corners are images served by
`JournalDocument::loadResource()` under an internal `hgs-md:` scheme (for example
`hgs-md:disc`, `hgs-md:corner-tl`). They are painted with `QPainter`
(antialiased) at the current content scale and device pixel ratio, cached per
theme and scale. Corner URLs carry their fill colour (`?fill=e8eef2`). Every other resource request still returns an empty image.

### Monospace font

Qt rich text does not resolve CSS font-family lists. The renderer picks the first
installed family of GitHub's stack (`ui-monospace`, `SFMono-Regular`, `SF Mono`,
`Menlo`, `Consolas`, `Liberation Mono`) via `QFontDatabase`, falling back to
`QFontDatabase::systemFont(FixedFont)`, and writes it on every code span: Qt
drops inherited font properties on nested coloured spans inside table cells.

## Copying

`ActivityView` uses a small `QTextBrowser` subclass whose
`createMimeDataFromSelection()` produces plain text: chip objects contribute their
code text, list markers contribute `• `/`◦ `/`▪ ` or the item number, checkboxes
`[ ] `/`[x] `, corner images nothing. No `U+FFFC` reaches the clipboard.

## Theme tokens (github-markdown-css 5.8.1)

| Token | Light | Dark |
| --- | --- | --- |
| `fg.default` | `#1f2328` | `#f0f6fc` |
| `fg.muted` | `#59636e` | `#9198a1` |
| `fg.accent` | `#0969da` | `#4493f8` |
| `canvas.default` | `#ffffff` | `#0d1117` |
| `canvas.subtle` | `#f6f8fa` | `#151b23` |
| `border.default` | `#d1d9e0` | `#3d444d` |
| `border.muted` | `#d1d9e0` at 70 % | `#3d444d` at 70 % |
| inline code fill | `#818b98` at 12 % | `#656c76` at 20 % |

Translucent tokens are composited over the canvas they are drawn on (for example
the dark chip fill is `#1f232a` over `#0d1117`).

## Testing

- `tray/tests/test_markdownhtml.cpp` (new): structure for every construct above,
  nesting (lists in quotes, code in lists, tables with inline code), ordered start
  numbers, task lists, `MD_FLAG_NOHTML` (raw HTML stays text), link policy
  (file, web, rejected, images), and that `_x_` is emphasis.
- `tray/tests/test_activityview.cpp`: update expectations that read the old
  `setMarkdown()` structure; add chip conversion, search-mode exemption, clean
  copy text, and resource loading for `hgs-md:` only.
- Visual check: render the prototype sample through `ActivityView` with the
  existing `HGS_*_PREVIEW` screenshot hooks in both themes and compare with the
  GitHub reference.
- Packaging: `scripts/package-linux.py` installs `md4c-LICENSE`;
  `scripts/check-package.py` requires it; `THIRD_PARTY_NOTICES.md` lists md4c.
