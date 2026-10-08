# GitHub-style Markdown in the Activity view — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Agent replies in the Activity view render Markdown like a GitHub comment body (light and dark), while the view stays a `QTextBrowser`.

**Architecture:** A new `MarkdownHtml` module parses Markdown with vendored md4c 0.5.2 and emits HTML restricted to what Qt rich text renders faithfully (tables for lists, quotes, code blocks and rules; explicit styles on every text run). `MarkdownObjects` draws the decorations (list markers, checkboxes, code-block corners) served by the journal document under `hgs-md:` and replaces inline-code runs with painted chip objects after `setHtml()`. `ActivityView` swaps its `setMarkdown()`-based helper for the new renderer and copies plain text without `U+FFFC`.

**Tech Stack:** C++17, Qt 6.11 (Widgets, rich text, `QTextObjectInterface`), md4c 0.5.2 (C99), CMake, QtTest.

**Spec:** `docs/superpowers/specs/2026-10-08-github-markdown-activity-design.md`

## Global Constraints

- Parser: md4c 0.5.2 vendored in `tray/vendor/md4c` (`md4c.c/h`, `entity.c/h`, `LICENSE.md`, `UPSTREAM.md` with SHA-256 `55d0111d48fb11883aaee91465e642b8b640775a4d6993c2d0e7a8092758ef21`). Flags `MD_DIALECT_GITHUB | MD_FLAG_NOHTML`; `MD_FLAG_UNDERLINE` stays off.
- The Activity view stays a `QTextBrowser`; no WebEngine, no syntax highlighting.
- Metrics: base font 14 px, block gap 16 px, heading/rule gap 24 px, line height 21 px (23 px for a block that contains inline code), code 12 px with 17 px line height. All lengths are emitted in `px` at content scale 1; `ContentScale::html()` scales them.
- Colours (github-markdown-css 5.8.1), light / dark: fg `#1f2328`/`#f0f6fc`, muted `#59636e`/`#9198a1`, accent `#0969da`/`#4493f8`, canvas `#ffffff`/`#0d1117`, subtle `#f6f8fa`/`#151b23`, border `#d1d9e0`/`#3d444d`, border.muted = border at 70 % over the canvas, chip = `#818b98` at 12 % / `#656c76` at 20 %.
- Link policy unchanged: valid `SessionFileReference` → `hgs-file:<sha256>` plus a visible ` (path:location)` suffix unless the label already shows it; `http`/`https` with a host and no user info stay links; everything else is plain text; images become “[Image attachment]”.
- Only `hgs-md:` resources load; every other resource request returns an empty image.
- Copied text contains no `U+FFFC`.
- Repository rules: commit directly on `main`, no Claude co-author line, stage only the files of the task (the tree contains unrelated in-progress edits), never restart the running GUI, agents or tmux.

## Review Focus

1. Long inline code (paths, commands) in a narrow pane: a chip cannot wrap, so a run longer than 40 characters must stay wrappable text with chip styling instead of being clipped (test in Task 4).
2. Content scale changes: resources, chip font and spacing must follow `setContentScale()` without stale cached images (test in Task 5).
3. Theme switch while a reply is visible: resources and colours must switch to dark without reusing light images (test in Task 5).
4. Deeply nested lists and quotes: no crash, bullets settle on squares and ordered markers on letters beyond depth 2 (test in Task 2).
5. CRLF input and tabs in code blocks: no stray `\r`, tabs shown as four spaces, indentation preserved (test in Task 3).

---

## File structure

| File | Responsibility |
| --- | --- |
| `tray/vendor/md4c/…` | Unmodified md4c parser and entity table, MIT. |
| `tray/cmake/Markdown.cmake` | Builds `hgs-md4c` (C) and `hgs-markdown` (Qt) static libraries. |
| `tray/src/MarkdownHtml.{h,cpp}` | Theme tokens and the Markdown → Qt rich-text HTML renderer. Pure strings. |
| `tray/src/MarkdownObjects.{h,cpp}` | Drawn `hgs-md:` images, inline-code chip objects and their conversion, plain-text extraction for copying. |
| `tray/src/ActivityView.{h,cpp}` | Uses the renderer, serves resources, converts chips after `setHtml()`, copies plain text. |
| `tray/tests/test_markdownhtml.cpp` | Renderer tests. |
| `tray/tests/test_markdownobjects.cpp` | Resource, chip and plain-text tests. |
| `tray/tests/test_activityview.cpp` | Integration expectations. |
| `scripts/package-linux.py`, `scripts/check-package.py`, `THIRD_PARTY_NOTICES.md` | md4c licence in packages and notices. |

Build directory used below: `tray/build` (already configured with `BUILD_TESTING=ON` and the Qt from `.deps`). After adding CMake files run `cmake -S tray -B tray/build` once.

---

### Task 1: Vendor md4c and render paragraphs, headings and inline text

**Files:**
- Create: `tray/vendor/md4c/src/{md4c.c,md4c.h,entity.c,entity.h}`, `tray/vendor/md4c/LICENSE.md`, `tray/vendor/md4c/UPSTREAM.md`
- Create: `tray/cmake/Markdown.cmake`, `tray/src/MarkdownHtml.h`, `tray/src/MarkdownHtml.cpp`, `tray/tests/test_markdownhtml.cpp`
- Modify: `tray/CMakeLists.txt` (include fragment, link `hgs-markdown` into `hgs-tray`), `tray/tests/CMakeLists.txt`
- Modify: `scripts/package-linux.py:57`, `scripts/check-package.py:20`, `THIRD_PARTY_NOTICES.md`

**Interfaces:**
- Produces:
  - `struct MarkdownTheme { bool dark; int scalePercent; QColor canvas, fg, muted, accent, subtle, border, borderMuted, chip; QString monoFamily; static MarkdownTheme github(bool dark, const QColor &canvas, double scale = 1.0); QString resource(const QString &name) const; }`
  - `struct MarkdownLink { QString href, location, tooltip; }`, `using MarkdownLinkPolicy = std::function<MarkdownLink(const QString &destination)>;`
  - `namespace MarkdownHtml { QColor chipSentinel(); QString monospaceFamily(); QString render(const QString &, const MarkdownTheme &, const MarkdownLinkPolicy &); }`
  - CMake targets `hgs-md4c`, `hgs-markdown` (AUTOMOC on, public include `tray/src`).

- [ ] **Step 1: Vendor md4c 0.5.2**

```bash
cd /home/n-prudnikov/w/zerus
tmp=$(mktemp -d)
curl -sfL -o "$tmp/md4c.tar.gz" https://github.com/mity/md4c/archive/refs/tags/release-0.5.2.tar.gz
echo "55d0111d48fb11883aaee91465e642b8b640775a4d6993c2d0e7a8092758ef21  $tmp/md4c.tar.gz" | sha256sum -c -
tar -xzf "$tmp/md4c.tar.gz" -C "$tmp"
mkdir -p tray/vendor/md4c/src
cp "$tmp"/md4c-release-0.5.2/src/{md4c.c,md4c.h,entity.c,entity.h} tray/vendor/md4c/src/
cp "$tmp"/md4c-release-0.5.2/LICENSE.md tray/vendor/md4c/LICENSE.md
rm -rf "$tmp"
```

Expected: `sha256sum` prints `OK`.

Create `tray/vendor/md4c/UPSTREAM.md`:

```markdown
Vendored md4c 0.5.2

Source: https://github.com/mity/md4c/archive/refs/tags/release-0.5.2.tar.gz
SHA-256: 55d0111d48fb11883aaee91465e642b8b640775a4d6993c2d0e7a8092758ef21

Original parser (`src/md4c.c`, `src/md4c.h`) and entity table (`src/entity.c`,
`src/entity.h`), unmodified. MIT license: LICENSE.md. Qt 6.11 bundles the same
version internally; Zerus uses its callback API to emit Qt rich-text HTML.
Builds offline.
```

- [ ] **Step 2: Add the CMake fragment and test target**

Create `tray/cmake/Markdown.cmake`:

```cmake
# Offline, pinned Markdown parser; see vendor/md4c/UPSTREAM.md.
enable_language(C)
set(HGS_MARKDOWN_ROOT "${CMAKE_CURRENT_LIST_DIR}/..")
set(HGS_MD4C_ROOT "${HGS_MARKDOWN_ROOT}/vendor/md4c")
add_library(hgs-md4c STATIC ${HGS_MD4C_ROOT}/src/md4c.c ${HGS_MD4C_ROOT}/src/entity.c)
set_target_properties(hgs-md4c PROPERTIES C_STANDARD 99 POSITION_INDEPENDENT_CODE ON)
target_include_directories(hgs-md4c PUBLIC ${HGS_MD4C_ROOT}/src)
add_library(hgs-markdown STATIC
    ${HGS_MARKDOWN_ROOT}/src/MarkdownHtml.h ${HGS_MARKDOWN_ROOT}/src/MarkdownHtml.cpp)
set_target_properties(hgs-markdown PROPERTIES AUTOMOC ON POSITION_INDEPENDENT_CODE ON)
target_include_directories(hgs-markdown PUBLIC ${HGS_MARKDOWN_ROOT}/src)
target_link_libraries(hgs-markdown PUBLIC Qt6::Widgets hgs-md4c)
```

In `tray/CMakeLists.txt` add after `include(cmake/Terminal.cmake)`:

```cmake
include(cmake/Markdown.cmake)
```

and add `hgs-markdown` to the `target_link_libraries(hgs-tray PRIVATE ...)` line (the one that lists `hgs-terminal`).

Append to `tray/tests/CMakeLists.txt`:

```cmake
add_executable(test_markdownhtml test_markdownhtml.cpp)
target_link_libraries(test_markdownhtml PRIVATE Qt6::Test hgs-markdown)
add_test(NAME markdownhtml COMMAND test_markdownhtml)
set_tests_properties(markdownhtml PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
```

- [ ] **Step 3: Write the failing tests**

Create `tray/tests/test_markdownhtml.cpp`:

```cpp
#include "MarkdownHtml.h"

#include <QFontDatabase>
#include <QTest>
#include <QTextBlock>
#include <QTextDocument>
#include <QTextFragment>

namespace {
const MarkdownTheme &light()
{
    static const MarkdownTheme theme = MarkdownTheme::github(false, QColor("#ffffff"));
    return theme;
}
QString html(const QString &markdown, const MarkdownLinkPolicy &links = {})
{
    return MarkdownHtml::render(markdown, light(), links);
}
// Qt keeps only part of any HTML; assertions about appearance go through a document.
std::unique_ptr<QTextDocument> document(const QString &markdown, const MarkdownLinkPolicy &links = {})
{
    auto result = std::make_unique<QTextDocument>();
    result->setHtml(html(markdown, links));
    return result;
}
QTextCharFormat formatOf(const QTextDocument &doc, const QString &text)
{
    for (auto block = doc.begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it)
            if (it.fragment().text().contains(text)) return it.fragment().charFormat();
    return {};
}
QStringList anchors(const QTextDocument &doc)
{
    QStringList result;
    for (auto block = doc.begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it)
            if (it.fragment().charFormat().isAnchor()) result.append(it.fragment().charFormat().anchorHref());
    return result;
}
MarkdownLink policy(const QString &destination)
{
    if (destination.startsWith("https://")) return {destination, {}, {}};
    if (destination == "notes.txt") return {"hgs-file:key", "notes.txt", "notes.txt"};
    return {};
}
}

class TestMarkdownHtml : public QObject {
    Q_OBJECT
private slots:
    void themeUsesGitHubTokens();
    void paragraphsUseGitHubMetrics();
    void emphasisFollowsGitHub();
    void rawHtmlStaysText();
    void headingsHaveGitHubSizes();
    void linksFollowPolicy();
    void imagesBecomePlaceholders();
    void entitiesAreDecoded();
    void inlineCodeUsesSentinelAndTallerLine();
    void monospaceFamilyIsInstalled();
};

void TestMarkdownHtml::themeUsesGitHubTokens()
{
    const auto dark = MarkdownTheme::github(true, QColor("#0d1117"), 1.5);
    QCOMPARE(light().fg.name(), QString("#1f2328")); QCOMPARE(dark.fg.name(), QString("#f0f6fc"));
    QCOMPARE(light().subtle.name(), QString("#f6f8fa")); QCOMPARE(dark.subtle.name(), QString("#151b23"));
    QVERIFY(qAbs(light().chip.alphaF() - 0.12) < 0.001); QVERIFY(qAbs(dark.chip.alphaF() - 0.2) < 0.001);
    QCOMPARE(dark.resource("disc"), QString("hgs-md:disc/dark/150"));
    QCOMPARE(light().resource("corner-tl"), QString("hgs-md:corner-tl/light/100"));
}

void TestMarkdownHtml::paragraphsUseGitHubMetrics()
{
    const auto output = html("One\n\nTwo");
    QVERIFY(output.contains("<p style=\"margin:0px 0 0 0;line-height:21px;\">"));
    QVERIFY(output.contains("<p style=\"margin:16px 0 0 0;line-height:21px;\">"));
    const auto doc = document("One\n\nTwo");
    const auto format = formatOf(*doc, "One");
    QCOMPARE(format.font().pixelSize(), 14);
    QCOMPARE(format.foreground().color().name(), QString("#1f2328"));
}

void TestMarkdownHtml::emphasisFollowsGitHub()
{
    const auto doc = document("_a_ *b* **c** ~~d~~");
    QVERIFY(formatOf(*doc, "a").fontItalic()); QVERIFY(!formatOf(*doc, "a").fontUnderline());
    QVERIFY(formatOf(*doc, "b").fontItalic());
    QCOMPARE(formatOf(*doc, "c").fontWeight(), 600);
    QVERIFY(formatOf(*doc, "d").fontStrikeOut());
}

void TestMarkdownHtml::rawHtmlStaysText()
{
    const auto doc = document("<b>bold</b> <script>run()</script>");
    QVERIFY(doc->toPlainText().contains("<b>bold</b> <script>run()</script>"));
    QVERIFY(formatOf(*doc, "bold").fontWeight() < 600);
}

void TestMarkdownHtml::headingsHaveGitHubSizes()
{
    const auto output = html("# A\n\n## B\n\n### C\n\n###### F");
    QCOMPARE(output.count("border-bottom:1px solid"), 2);
    const auto doc = document("# A\n\n## B\n\n### C\n\n###### F");
    QCOMPARE(formatOf(*doc, "A").font().pixelSize(), 28);
    QCOMPARE(formatOf(*doc, "B").font().pixelSize(), 21);
    QCOMPARE(formatOf(*doc, "C").font().pixelSize(), 18);
    QCOMPARE(formatOf(*doc, "F").foreground().color().name(), QString("#59636e"));
    QCOMPARE(formatOf(*doc, "A").fontWeight(), 600);
    QVERIFY(output.contains("margin-top:24px"));
}

void TestMarkdownHtml::linksFollowPolicy()
{
    const QString text = "[ok](https://example.com) [bad](javascript:x) [Label](notes.txt) [notes.txt](notes.txt) <https://example.org>";
    const auto doc = document(text, policy);
    const auto links = anchors(*doc);
    QVERIFY(links.contains("https://example.com")); QVERIFY(links.contains("https://example.org"));
    QVERIFY(links.contains("hgs-file:key")); QVERIFY(!links.contains("javascript:x"));
    QVERIFY(doc->toPlainText().contains("bad"));
    QVERIFY(doc->toPlainText().contains("Label (notes.txt)"));
    QCOMPARE(doc->toPlainText().count("notes.txt"), 2);
    QCOMPARE(formatOf(*doc, "ok").foreground().color().name(), QString("#0969da"));
    QVERIFY(!formatOf(*doc, "ok").fontUnderline());
    QCOMPARE(formatOf(*doc, "Label").toolTip(), QString("notes.txt"));
}

void TestMarkdownHtml::imagesBecomePlaceholders()
{
    const auto doc = document("![secret alt](https://example.com/tracker.png)", policy);
    QVERIFY(doc->toPlainText().contains("[Image attachment]"));
    QVERIFY(!doc->toPlainText().contains("secret alt"));
    for (auto block = doc->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(!it.fragment().charFormat().isImageFormat());
}

void TestMarkdownHtml::entitiesAreDecoded()
{
    QVERIFY(document("&amp; &copy; &#65; &#x42; &bogus;")->toPlainText().contains(QString::fromUtf8("& © A B &bogus;")));
}

void TestMarkdownHtml::inlineCodeUsesSentinelAndTallerLine()
{
    const auto output = html("Run `a  b` now");
    QVERIFY(output.contains("background-color:#010203;"));
    QVERIFY(output.contains("a&nbsp;&nbsp;b"));
    QVERIFY(output.contains("line-height:23px;"));
    QCOMPARE(MarkdownHtml::chipSentinel(), QColor(1, 2, 3));
}

void TestMarkdownHtml::monospaceFamilyIsInstalled()
{
    const auto family = MarkdownHtml::monospaceFamily();
    QVERIFY(!family.isEmpty());
    QVERIFY(QFontDatabase::families().contains(family, Qt::CaseInsensitive)
            || family == QFontDatabase::systemFont(QFontDatabase::FixedFont).family());
    QCOMPARE(light().monoFamily, family);
}

QTEST_MAIN(TestMarkdownHtml)
#include "test_markdownhtml.moc"
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cmake -S tray -B tray/build && make -C tray/build -j8 test_markdownhtml`
Expected: compile error, `MarkdownHtml.h` / `MarkdownHtml.cpp` missing.

- [ ] **Step 5: Implement `MarkdownHtml.h`**

```cpp
#pragma once

#include <QColor>
#include <QString>
#include <functional>

// GitHub comment styling (github-markdown-css 5.8.1) for agent replies, expressed
// in the rich-text subset QTextDocument renders. Lengths are px at content scale 1.
struct MarkdownTheme {
    bool dark = false;
    int scalePercent = 100;   // keys the drawn hgs-md: resources
    QColor canvas, fg, muted, accent, subtle, border, borderMuted, chip;
    QString monoFamily;

    static MarkdownTheme github(bool dark, const QColor &canvas, double scale = 1.0);
    // "hgs-md:<name>/<light|dark>/<scale percent>", drawn by MarkdownObjects.
    QString resource(const QString &name) const;
};

// A link the renderer may emit. An empty href renders the label as plain text.
// A location is shown as " (location)" unless the label already reads so.
struct MarkdownLink {
    QString href, location, tooltip;
};
using MarkdownLinkPolicy = std::function<MarkdownLink(const QString &destination)>;

namespace MarkdownHtml {
// Inline code is emitted with this background; MarkdownObjects turns each run into a chip.
QColor chipSentinel();
// First installed family of GitHub's monospace stack.
QString monospaceFamily();
QString render(const QString &markdown, const MarkdownTheme &theme, const MarkdownLinkPolicy &links);
}
```

- [ ] **Step 6: Implement `MarkdownHtml.cpp`**

```cpp
#include "MarkdownHtml.h"

#include <QFontDatabase>
#include <QObject>
#include <QStringList>
#include <QVector>

#include <md4c.h>
extern "C" {
#include <entity.h>
}

namespace {
// github-markdown-css 5.8.1 comment metrics at a 14 px base font (rem = 16 px).
constexpr int BaseSize = 14, CodeSize = 12;
constexpr int BlockGap = 16, HeadingGap = 24, RuleGap = 24;
constexpr int LineHeight = 21, CodeLineHeight = 23;

QColor over(const QColor &top, double alpha, const QColor &bottom)
{
    const auto mix = [alpha](float a, float b) { return a * alpha + b * (1 - alpha); };
    return QColor::fromRgbF(mix(top.redF(), bottom.redF()), mix(top.greenF(), bottom.greenF()), mix(top.blueF(), bottom.blueF()));
}

QString codepoint(uint value)
{
    if (value == 0 || value > 0x10FFFF || (value >= 0xD800 && value <= 0xDFFF)) return QString(QChar(QChar::ReplacementCharacter));
    const char32_t character = value;
    return QString::fromUcs4(&character, 1);
}

// "&amp;", "&#65;" or "&#x42;" as reported by md4c (MD_TEXT_ENTITY).
QString decodeEntity(const QByteArray &entity)
{
    if (entity.startsWith("&#")) {
        const bool hex = entity.size() > 3 && (entity[2] == 'x' || entity[2] == 'X');
        bool ok = false;
        const uint value = entity.mid(hex ? 3 : 2, entity.size() - (hex ? 4 : 3)).toUInt(&ok, hex ? 16 : 10);
        return ok ? codepoint(value) : QString(QChar(QChar::ReplacementCharacter));
    }
    const ENTITY *found = entity_lookup(entity.constData(), size_t(entity.size()));
    if (!found) return QString::fromUtf8(entity);
    return codepoint(found->codepoints[0]) + (found->codepoints[1] ? codepoint(found->codepoints[1]) : QString());
}

QString attribute(const MD_ATTRIBUTE &value)
{
    QString result;
    for (int i = 0; value.substr_offsets[i] < value.size; ++i) {
        const QByteArray part(value.text + value.substr_offsets[i], qsizetype(value.substr_offsets[i + 1] - value.substr_offsets[i]));
        if (value.substr_types[i] == MD_TEXT_NULLCHAR) result += QChar(QChar::ReplacementCharacter);
        else if (value.substr_types[i] == MD_TEXT_ENTITY) result += decodeEntity(part);
        else result += QString::fromUtf8(part);
    }
    return result;
}

int headingSize(unsigned level)
{
    static constexpr int sizes[] = {28, 21, 18, 14, 12, 12};
    return sizes[qBound(1u, level, 6u) - 1];
}

// Inline formatting is written on every text run: Qt drops inherited font
// properties on nested spans inside table cells.
struct Style {
    int size = BaseSize;
    bool bold = false, italic = false, strike = false;
    QColor color;
    QString family;
    QString css() const
    {
        QString result = QString("font-size:%1px;").arg(size);
        if (bold) result += "font-weight:600;";
        if (italic) result += "font-style:italic;";
        if (strike) result += "text-decoration:line-through;";
        if (color.isValid()) result += QString("color:%1;").arg(color.name());
        if (!family.isEmpty()) result += QString("font-family:'%1';").arg(family);
        return result;
    }
};

struct Frame {
    MD_BLOCKTYPE type = MD_BLOCK_DOC;
    QString html, raw;
    int blocks = 0;          // child blocks (or list rows) emitted so far
    bool afterRule = false;  // the previous child was a thematic break
    bool code = false;       // contains inline code
    bool ordered = false, loose = false, task = false, checked = false, head = false;
    unsigned number = 1, level = 0;
    int depth = 0, bodyRow = 0;
    MD_ALIGN align = MD_ALIGN_DEFAULT;
};

struct Link {
    qsizetype start = 0;
    QString label, destination;
    MarkdownLink target;
};

class Renderer {
public:
    Renderer(const MarkdownTheme &theme, const MarkdownLinkPolicy &links) : m_theme(theme), m_links(links)
    {
        m_frames.append(Frame{});
        Style base; base.color = theme.fg; m_styles.append(base);
    }
    QString html() const { return m_frames.first().html; }

    int enterBlock(MD_BLOCKTYPE type, void *detail)
    {
        if (type == MD_BLOCK_DOC) return 0;
        Frame frame; frame.type = type;
        switch (type) {
        case MD_BLOCK_H: {
            frame.level = static_cast<MD_BLOCK_H_DETAIL *>(detail)->level;
            const int size = headingSize(frame.level); const bool muted = frame.level == 6;
            pushStyle([&](Style &s) { s.size = size; s.bold = true; if (muted) s.color = m_theme.muted; });
            break;
        }
        default: break;
        }
        m_frames.append(frame);
        return 0;
    }

    int leaveBlock(MD_BLOCKTYPE type, void *)
    {
        if (type == MD_BLOCK_DOC) return 0;
        const Frame frame = m_frames.takeLast();
        switch (type) {
        case MD_BLOCK_P:
            append(QString("<p style=\"margin:%1px 0 0 0;line-height:%2px;\">").arg(gap(BlockGap)).arg(frame.code ? CodeLineHeight : LineHeight)
                   + frame.html + "</p>");
            break;
        case MD_BLOCK_H: {
            m_styles.removeLast();
            const int size = headingSize(frame.level), lineHeight = qRound(size * 1.25), top = gap(HeadingGap);
            if (frame.level <= 2)
                append(QString("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:%1px;border-collapse:collapse;\">"
                               "<tr><td style=\"padding-bottom:%2px;border-bottom:1px solid %3;line-height:%4px;\">")
                           .arg(top).arg(qRound(size * 0.3)).arg(m_theme.borderMuted.name()).arg(lineHeight)
                       + frame.html + "</td></tr></table>");
            else append(QString("<p style=\"margin:%1px 0 0 0;line-height:%2px;\">").arg(top).arg(lineHeight) + frame.html + "</p>");
            break;
        }
        default:
            // Blocks without their own presentation keep their content.
            m_frames.last().html += frame.html;
            break;
        }
        return 0;
    }

    int enterSpan(MD_SPANTYPE type, void *detail)
    {
        switch (type) {
        case MD_SPAN_EM: pushStyle([](Style &s) { s.italic = true; }); break;
        case MD_SPAN_STRONG: pushStyle([](Style &s) { s.bold = true; }); break;
        case MD_SPAN_DEL: pushStyle([](Style &s) { s.strike = true; }); break;
        case MD_SPAN_CODE:
            m_inCode = true; m_frames.last().code = true;
            m_frames.last().html += QString("<span style=\"background-color:%1;\">").arg(MarkdownHtml::chipSentinel().name());
            break;
        case MD_SPAN_A: {
            Link link; link.start = m_frames.last().html.size();
            link.destination = attribute(static_cast<MD_SPAN_A_DETAIL *>(detail)->href);
            if (m_links) link.target = m_links(link.destination);
            const bool active = !link.target.href.isEmpty();
            m_linkStack.append(link);
            pushStyle([&](Style &s) { if (active) s.color = m_theme.accent; });
            break;
        }
        case MD_SPAN_IMG:
            if (!m_hidden) m_frames.last().html += run(QObject::tr("[Image attachment]"));
            ++m_hidden;
            break;
        default: break;
        }
        return 0;
    }

    int leaveSpan(MD_SPANTYPE type)
    {
        switch (type) {
        case MD_SPAN_EM: case MD_SPAN_STRONG: case MD_SPAN_DEL: m_styles.removeLast(); break;
        case MD_SPAN_CODE: m_inCode = false; m_frames.last().html += "</span>"; break;
        case MD_SPAN_A: {
            m_styles.removeLast();
            const Link link = m_linkStack.takeLast();
            if (link.target.href.isEmpty()) break;
            QString &html = m_frames.last().html;
            const QString &location = link.target.location;
            if (!location.isEmpty() && link.label != location && link.label != link.destination) {
                Style style = m_styles.last(); style.color = m_theme.accent;
                html += QString("<span style=\"%1\">").arg(style.css()) + (" (" + location + ")").toHtmlEscaped() + "</span>";
            }
            html.insert(link.start, QString("<a href=\"%1\" title=\"%2\" style=\"color:%3;text-decoration:none;\">")
                                        .arg(link.target.href.toHtmlEscaped(), link.target.tooltip.toHtmlEscaped(), m_theme.accent.name()));
            html += "</a>";
            break;
        }
        case MD_SPAN_IMG: --m_hidden; break;
        default: break;
        }
        return 0;
    }

    int text(MD_TEXTTYPE type, const MD_CHAR *data, MD_SIZE size)
    {
        if (m_hidden) return 0;
        Frame &frame = m_frames.last();
        if (type == MD_TEXT_BR) { frame.html += m_inCode ? QStringLiteral("&nbsp;") : QStringLiteral("<br>"); return 0; }
        QString value;
        if (type == MD_TEXT_NULLCHAR) value = QChar(QChar::ReplacementCharacter);
        else if (type == MD_TEXT_SOFTBR) value = QStringLiteral(" ");
        else if (type == MD_TEXT_ENTITY) value = decodeEntity(QByteArray(data, qsizetype(size)));
        else value = QString::fromUtf8(data, qsizetype(size));
        if (!m_linkStack.isEmpty()) m_linkStack.last().label += value;
        if (m_inCode) frame.html += value.toHtmlEscaped().replace(' ', QStringLiteral("&nbsp;"));
        else frame.html += run(value);
        return 0;
    }

private:
    void pushStyle(const std::function<void(Style &)> &change) { Style style = m_styles.last(); change(style); m_styles.append(style); }
    // The first block of a container has no gap; a block after a rule keeps the rule's gap.
    int gap(int wanted) const { const Frame &c = m_frames.last(); return c.blocks == 0 ? 0 : c.afterRule ? RuleGap : wanted; }
    void append(const QString &html, bool rule = false) { Frame &c = m_frames.last(); c.html += html; ++c.blocks; c.afterRule = rule; }
    QString run(const QString &value) const { return QString("<span style=\"%1\">").arg(m_styles.last().css()) + value.toHtmlEscaped() + "</span>"; }

    const MarkdownTheme &m_theme;
    const MarkdownLinkPolicy &m_links;
    QVector<Frame> m_frames;
    QVector<Style> m_styles;
    QVector<Link> m_linkStack;
    int m_hidden = 0;   // inside image alt text
    bool m_inCode = false;
};
}

MarkdownTheme MarkdownTheme::github(bool dark, const QColor &canvas, double scale)
{
    MarkdownTheme theme;
    theme.dark = dark; theme.scalePercent = qRound(scale * 100); theme.canvas = canvas;
    theme.fg = QColor(dark ? "#f0f6fc" : "#1f2328");
    theme.muted = QColor(dark ? "#9198a1" : "#59636e");
    theme.accent = QColor(dark ? "#4493f8" : "#0969da");
    theme.subtle = QColor(dark ? "#151b23" : "#f6f8fa");
    theme.border = QColor(dark ? "#3d444d" : "#d1d9e0");
    theme.borderMuted = over(theme.border, 0.7, canvas);
    theme.chip = QColor(dark ? "#656c76" : "#818b98"); theme.chip.setAlphaF(dark ? 0.2f : 0.12f);
    theme.monoFamily = MarkdownHtml::monospaceFamily();
    return theme;
}

QString MarkdownTheme::resource(const QString &name) const
{
    return QString("hgs-md:%1/%2/%3").arg(name, dark ? QStringLiteral("dark") : QStringLiteral("light"), QString::number(scalePercent));
}

QColor MarkdownHtml::chipSentinel() { return QColor(1, 2, 3); }

QString MarkdownHtml::monospaceFamily()
{
    static const QString family = [] {
        const QStringList installed = QFontDatabase::families();
        for (const char *candidate : {"ui-monospace", "SFMono-Regular", "SF Mono", "Menlo", "Consolas", "Liberation Mono"})
            if (installed.contains(QLatin1String(candidate), Qt::CaseInsensitive)) return QString::fromLatin1(candidate);
        return QFontDatabase::systemFont(QFontDatabase::FixedFont).family();
    }();
    return family;
}

QString MarkdownHtml::render(const QString &markdown, const MarkdownTheme &theme, const MarkdownLinkPolicy &links)
{
    Renderer renderer(theme, links);
    MD_PARSER parser{};
    parser.abi_version = 0;
    // GitHub semantics: `_x_` is emphasis, so MD_FLAG_UNDERLINE stays off. Raw HTML remains text.
    parser.flags = MD_DIALECT_GITHUB | MD_FLAG_NOHTML;
    parser.enter_block = [](MD_BLOCKTYPE type, void *detail, void *self) { return static_cast<Renderer *>(self)->enterBlock(type, detail); };
    parser.leave_block = [](MD_BLOCKTYPE type, void *detail, void *self) { return static_cast<Renderer *>(self)->leaveBlock(type, detail); };
    parser.enter_span = [](MD_SPANTYPE type, void *detail, void *self) { return static_cast<Renderer *>(self)->enterSpan(type, detail); };
    parser.leave_span = [](MD_SPANTYPE type, void *, void *self) { return static_cast<Renderer *>(self)->leaveSpan(type); };
    parser.text = [](MD_TEXTTYPE type, const MD_CHAR *text, MD_SIZE size, void *self) { return static_cast<Renderer *>(self)->text(type, text, size); };
    const QByteArray source = markdown.toUtf8();
    if (md_parse(source.constData(), MD_SIZE(source.size()), &parser, &renderer) != 0)
        return "<p>" + markdown.toHtmlEscaped() + "</p>";
    return renderer.html();
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `make -C tray/build -j8 test_markdownhtml && QT_QPA_PLATFORM=offscreen tray/build/tests/test_markdownhtml`
Expected: all 10 tests PASS. If `paragraphsUseGitHubMetrics` reports a different pixel size, print `html("One")` and check that the run span carries `font-size:14px;`.

- [ ] **Step 8: Ship the licence**

In `scripts/package-linux.py` after the libvterm line add:

```python
    install(ROOT / "tray/vendor/md4c/LICENSE.md", licenses / "md4c-LICENSE")
```

In `scripts/check-package.py` extend `required` with `f"usr/share/licenses/{args.package_name}/md4c-LICENSE"` next to the libvterm entry.

In `THIRD_PARTY_NOTICES.md`, under “Vendored code”, add:

```markdown
`tray/vendor/md4c` contains the md4c 0.5.2 Markdown parser and entity table
under MIT. Its original license is retained in `tray/vendor/md4c/LICENSE.md`;
source provenance and archive checksum are in `UPSTREAM.md` beside it.
```

and change “alongside the Zerus and vendored libvterm licenses” to “alongside the Zerus and vendored libvterm and md4c licenses”.

Run: `python3 scripts/check-source-privacy.py`
Expected: only the pre-existing `.beads/issues.jsonl` finding, nothing under `tray/vendor/md4c`.

- [ ] **Step 9: Commit**

```bash
git add tray/vendor/md4c tray/cmake/Markdown.cmake tray/src/MarkdownHtml.h tray/src/MarkdownHtml.cpp \
        tray/tests/test_markdownhtml.cpp tray/CMakeLists.txt tray/tests/CMakeLists.txt \
        scripts/package-linux.py scripts/check-package.py THIRD_PARTY_NOTICES.md
git commit -m "Render agent Markdown paragraphs and headings like GitHub with md4c"
```

---

### Task 2: Lists and task items

**Files:**
- Modify: `tray/src/MarkdownHtml.cpp` (Renderer: list cases, `image()`, `ordinal()`)
- Test: `tray/tests/test_markdownhtml.cpp`

**Interfaces:**
- Consumes: `Renderer`, `Frame`, `gap()`, `append()`, `run()`, `MarkdownTheme::resource()` from Task 1.
- Produces: list HTML — one two-column table per list, marker cell `width="28"`; markers `hgs-md:disc|circle|square|check-on|check-off/<theme>/<scale>` as `<img ... height="19">`; ordered markers as text `N.`, `i.`, `a.`. Task 4's plain-text extraction relies on this shape (column 0 of a two-column table holds only the marker).

- [ ] **Step 1: Write the failing tests**

Add slots `bulletListsUseDrawnMarkers`, `orderedListsHonourStartAndNesting`, `looseListItemsAreSpaced`, `taskItemsShowCheckboxes`, `deepNestingSettles` to the class and implement:

```cpp
void TestMarkdownHtml::bulletListsUseDrawnMarkers()
{
    const auto output = html("Intro\n\n- a\n- b\n  - c\n    - d");
    QVERIFY(output.contains(light().resource("disc"))); QVERIFY(output.contains(light().resource("circle")));
    QVERIFY(output.contains(light().resource("square")));
    QVERIFY(output.contains("<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:16px;\">"));
    QVERIFY(output.contains("<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:0px;\">"));
    QVERIFY(output.contains("padding:0px 12px 0 0;line-height:21px;"));
    QVERIFY(output.contains("padding:3px 12px 0 0;line-height:21px;"));
    QVERIFY(document("- a\n- b")->toPlainText().contains("a"));
}

void TestMarkdownHtml::orderedListsHonourStartAndNesting()
{
    const auto plain = document("3. a\n4. b\n   1. c\n      1. d")->toPlainText();
    for (const char *marker : {"3.", "4.", "i.", "a."}) QVERIFY2(plain.contains(QLatin1String(marker)), marker);
    QVERIFY(html("1. a\n2. b").contains("padding:3px 4px 0 0;"));
}

void TestMarkdownHtml::looseListItemsAreSpaced()
{
    QVERIFY(html("- a\n\n- b").contains("padding:16px 12px 0 0;"));
    QVERIFY(html("- `code` item").contains("line-height:23px;"));
}

void TestMarkdownHtml::taskItemsShowCheckboxes()
{
    const auto output = html("- [ ] todo\n- [x] done");
    QVERIFY(output.contains(light().resource("check-off"))); QVERIFY(output.contains(light().resource("check-on")));
    QVERIFY(!output.contains(light().resource("disc")));
    QVERIFY(document("- [ ] todo")->toPlainText().contains("todo"));
}

void TestMarkdownHtml::deepNestingSettles()
{
    QString markdown;
    for (int depth = 0; depth < 10; ++depth) markdown += QString(depth * 2, ' ') + "- level\n";
    const auto output = html(markdown);
    QCOMPARE(output.count(light().resource("square")), 8);
    QString ordered;
    for (int depth = 0; depth < 5; ++depth) ordered += QString(depth * 3, ' ') + "1. level\n";
    QVERIFY(document(ordered)->toPlainText().contains("a."));
    QVERIFY(document("> > > - deep\n> > > > quote")->toPlainText().contains("deep"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `make -C tray/build -j8 test_markdownhtml && QT_QPA_PLATFORM=offscreen tray/build/tests/test_markdownhtml`
Expected: the five new tests FAIL (no marker resources in the output).

- [ ] **Step 3: Implement lists**

In `Renderer::enterBlock`, add before `default:`:

```cpp
        case MD_BLOCK_UL:
            frame.loose = !static_cast<MD_BLOCK_UL_DETAIL *>(detail)->is_tight;
            frame.depth = listDepth();
            break;
        case MD_BLOCK_OL: {
            const auto *list = static_cast<MD_BLOCK_OL_DETAIL *>(detail);
            frame.ordered = true; frame.loose = !list->is_tight; frame.number = list->start; frame.depth = listDepth();
            break;
        }
        case MD_BLOCK_LI: {
            const auto *item = static_cast<MD_BLOCK_LI_DETAIL *>(detail);
            frame.task = item->is_task;
            frame.checked = item->is_task && (item->task_mark == 'x' || item->task_mark == 'X');
            break;
        }
```

In `Renderer::leaveBlock`, add before `default:`:

```cpp
        case MD_BLOCK_UL: case MD_BLOCK_OL: {
            // A nested list continues its item without a gap (GitHub: ul ul { margin: 0 }).
            const int top = m_frames.last().type == MD_BLOCK_LI ? 0 : gap(BlockGap);
            append(QString("<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:%1px;\">").arg(top) + frame.html + "</table>");
            break;
        }
        case MD_BLOCK_LI: {
            Frame &list = m_frames.last();
            const int top = list.blocks == 0 ? 0 : list.loose ? BlockGap : 3;
            const int lineHeight = frame.code ? CodeLineHeight : LineHeight;
            QString marker; int space = 12;
            if (frame.task) { marker = image(frame.checked ? "check-on" : "check-off", 13); space = 7; }
            else if (list.ordered) { marker = run(ordinal(list.number, list.depth) + '.'); space = 4; }
            else marker = image(list.depth == 0 ? "disc" : list.depth == 1 ? "circle" : "square", 5);
            list.html += QString("<tr><td width=\"28\" align=\"right\" valign=\"top\" style=\"padding:%1px %2px 0 0;line-height:%3px;\">")
                             .arg(top).arg(space).arg(lineHeight)
                       + marker
                       + QString("</td><td valign=\"top\" style=\"padding-top:%1px;line-height:%2px;\">").arg(top).arg(lineHeight)
                       + frame.html + "</td></tr>";
            ++list.blocks; ++list.number;
            break;
        }
```

Add these private helpers to `Renderer`:

```cpp
    int listDepth() const
    {
        int depth = 0;
        for (const auto &frame : m_frames) depth += frame.type == MD_BLOCK_UL || frame.type == MD_BLOCK_OL;
        return depth;
    }
    // Marker images are 19 px tall so that Qt's baseline alignment puts the mark where GitHub does.
    QString image(const char *name, int width) const
    {
        return QString("<img src=\"%1\" width=\"%2\" height=\"19\">").arg(m_theme.resource(QLatin1String(name))).arg(width);
    }
    // GitHub: ol → decimal, nested → lower-roman, deeper → lower-alpha.
    static QString ordinal(unsigned number, int depth)
    {
        if (depth == 0) return QString::number(number);
        if (depth == 1) {
            static const QList<QPair<unsigned, const char *>> numerals{{1000, "m"}, {900, "cm"}, {500, "d"}, {400, "cd"}, {100, "c"},
                {90, "xc"}, {50, "l"}, {40, "xl"}, {10, "x"}, {9, "ix"}, {5, "v"}, {4, "iv"}, {1, "i"}};
            QString result;
            for (const auto &[value, text] : numerals) while (number >= value) { result += QLatin1String(text); number -= value; }
            return result;
        }
        QString result;
        while (number > 0) { --number; result.prepend(QChar('a' + int(number % 26))); number /= 26; }
        return result;
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `make -C tray/build -j8 test_markdownhtml && QT_QPA_PLATFORM=offscreen tray/build/tests/test_markdownhtml`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add tray/src/MarkdownHtml.cpp tray/tests/test_markdownhtml.cpp
git commit -m "Lay out Markdown lists with GitHub markers and spacing"
```

---

### Task 3: Code blocks, quotes, tables and rules

**Files:**
- Modify: `tray/src/MarkdownHtml.cpp` (Renderer: quote, code, table, rule cases, `codeBlock()`, `preserveSpaces()`, code text branch)
- Test: `tray/tests/test_markdownhtml.cpp`

**Interfaces:**
- Consumes: Task 1 renderer, Task 2 helpers.
- Produces: code blocks as a 3×3 table whose corner cells hold `hgs-md:corner-tl|tr|bl|br` 6×6 images and whose edge cells hold only `&#8203;`; Task 4 skips both when copying.

- [ ] **Step 1: Write the failing tests**

Add slots `codeBlocksKeepWhitespaceAndRoundCorners`, `codeBlocksIgnoreCarriageReturns`, `blockquotesHaveBarAndMutedText`, `tablesHaveBordersZebraAndAlignment`, `rulesUseGitHubGaps`:

```cpp
void TestMarkdownHtml::codeBlocksKeepWhitespaceAndRoundCorners()
{
    const QString markdown = "```python\nif x:\n    y  = 1   # note\n```";
    const auto output = html(markdown);
    for (const char *corner : {"corner-tl", "corner-tr", "corner-bl", "corner-br"})
        QVERIFY2(output.contains(light().resource(QLatin1String(corner))), corner);
    QVERIFY(output.contains("padding:7px 10px 13px 10px;line-height:17px;"));
    const auto doc = document(markdown);
    QVERIFY(doc->toPlainText().contains("    y  = 1   # note"));
    const auto format = formatOf(*doc, "if x:");
    QCOMPARE(format.font().pixelSize(), 12);
    QCOMPARE(format.fontFamilies().toStringList().value(0), light().monoFamily);
}

void TestMarkdownHtml::codeBlocksIgnoreCarriageReturns()
{
    const auto plain = document("```\r\na\tb\r\nc\r\n```")->toPlainText();
    QVERIFY(!plain.contains('\r'));
    QVERIFY(plain.contains("a    b"));
    QVERIFY(plain.contains("c"));
}

void TestMarkdownHtml::blockquotesHaveBarAndMutedText()
{
    QVERIFY(html("> quoted").contains("<td width=\"4\" bgcolor=\"#d1d9e0\"></td>"));
    QCOMPARE(formatOf(*document("> quoted"), "quoted").foreground().color().name(), QString("#59636e"));
}

void TestMarkdownHtml::tablesHaveBordersZebraAndAlignment()
{
    const QString markdown = "| A | B |\n|---|--:|\n| 1 | `x` |\n| 3 | 4 |\n| 5 | 6 |";
    const auto output = html(markdown);
    QVERIFY(output.contains("border-collapse:collapse;"));
    QCOMPARE(output.count("<tr bgcolor=\"#ffffff\">"), 3);
    QCOMPARE(output.count("<tr bgcolor=\"#f6f8fa\">"), 1);
    QVERIFY(output.contains("<th align=\"center\""));
    QVERIFY(output.contains("<td align=\"right\""));
    QVERIFY(output.contains("line-height:23px;"));
    QCOMPARE(formatOf(*document(markdown), "A").fontWeight(), 600);
}

void TestMarkdownHtml::rulesUseGitHubGaps()
{
    const auto output = html("a\n\n---\n\nb");
    QVERIFY(output.contains("bgcolor=\"#d1d9e0\" style=\"margin-top:24px;\""));
    QVERIFY(output.contains("height=\"4\""));
    QVERIFY(output.contains("<p style=\"margin:24px 0 0 0;line-height:21px;\">"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `make -C tray/build -j8 test_markdownhtml && QT_QPA_PLATFORM=offscreen tray/build/tests/test_markdownhtml`
Expected: the five new tests FAIL.

- [ ] **Step 3: Implement**

In `Renderer::enterBlock`, add before `default:`:

```cpp
        case MD_BLOCK_QUOTE:
            pushStyle([&](Style &s) { s.color = m_theme.muted; });
            break;
        case MD_BLOCK_TH:
            pushStyle([](Style &s) { s.bold = true; });
            frame.align = static_cast<MD_BLOCK_TD_DETAIL *>(detail)->align;
            break;
        case MD_BLOCK_TD:
            frame.align = static_cast<MD_BLOCK_TD_DETAIL *>(detail)->align;
            break;
        case MD_BLOCK_TR:
            frame.head = m_frames.last().type == MD_BLOCK_THEAD;
            break;
```

In `Renderer::leaveBlock`, add before `default:`:

```cpp
        case MD_BLOCK_QUOTE:
            m_styles.removeLast();
            append(QString("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:%1px;\"><tr>"
                           "<td width=\"4\" bgcolor=\"%2\"></td><td style=\"padding:0 14px;\">").arg(gap(BlockGap)).arg(m_theme.border.name())
                   + frame.html + "</td></tr></table>");
            break;
        case MD_BLOCK_CODE:
            append(codeBlock(frame.raw, gap(BlockGap)));
            break;
        case MD_BLOCK_HR:
            append(QString("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" bgcolor=\"%1\" style=\"margin-top:%2px;\">"
                           "<tr><td height=\"4\" style=\"font-size:1px;line-height:4px;\">&#8203;</td></tr></table>")
                       .arg(m_theme.border.name()).arg(gap(RuleGap)), true);
            break;
        case MD_BLOCK_TABLE:
            append(QString("<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:%1px;border-collapse:collapse;\">").arg(gap(BlockGap))
                   + frame.html + "</table>");
            break;
        case MD_BLOCK_THEAD: case MD_BLOCK_TBODY:
            m_frames.last().html += frame.html;
            break;
        case MD_BLOCK_TR: {
            Frame *table = nullptr;
            for (auto i = m_frames.size() - 1; i >= 0 && !table; --i) if (m_frames[i].type == MD_BLOCK_TABLE) table = &m_frames[i];
            const bool zebra = !frame.head && table && table->bodyRow++ % 2 == 1;
            m_frames.last().html += QString("<tr bgcolor=\"%1\">").arg((zebra ? m_theme.subtle : m_theme.canvas).name()) + frame.html + "</tr>";
            break;
        }
        case MD_BLOCK_TH: case MD_BLOCK_TD: {
            const bool header = type == MD_BLOCK_TH;
            if (header) m_styles.removeLast();
            const QString tag = header ? QStringLiteral("th") : QStringLiteral("td");
            // Browsers centre header cells unless the column says otherwise.
            const QString align = frame.align == MD_ALIGN_LEFT ? "left" : frame.align == MD_ALIGN_CENTER ? "center"
                : frame.align == MD_ALIGN_RIGHT ? "right" : header ? "center" : "";
            m_frames.last().html += "<" + tag + (align.isEmpty() ? QString() : " align=\"" + align + "\"")
                + QString(" style=\"padding:6px 13px;border:1px solid %1;line-height:%2px;\">").arg(m_theme.border.name())
                      .arg(frame.code ? CodeLineHeight : LineHeight)
                + frame.html + "</" + tag + ">";
            break;
        }
```

In `Renderer::text`, directly after the label line (`if (!m_linkStack.isEmpty()) ...`), add:

```cpp
        if (frame.type == MD_BLOCK_CODE) { frame.raw += value; return 0; }
```

Add the private helpers:

```cpp
    // Runs, indentation and trailing spaces survive HTML whitespace collapsing;
    // a single space between words stays breakable for narrow panes.
    static QString preserveSpaces(const QString &escaped)
    {
        QString result;
        for (qsizetype i = 0; i < escaped.size(); ++i) {
            const bool space = escaped[i] == ' ';
            const bool lone = space && i > 0 && escaped[i - 1] != ' ' && i + 1 < escaped.size() && escaped[i + 1] != ' ';
            result += space && !lone ? QStringLiteral("&nbsp;") : QString(escaped[i]);
        }
        return result;
    }
    // Rounded corners: a 3×3 table with drawn 6 px corners around the padded code.
    QString codeBlock(QString code, int top) const
    {
        code.remove('\r'); code.replace('\t', QStringLiteral("    "));
        if (code.endsWith('\n')) code.chop(1);
        Style style; style.size = CodeSize; style.color = m_theme.fg; style.family = m_theme.monoFamily;
        QStringList lines;
        for (const QString &line : code.split('\n'))
            lines.append(QString("<span style=\"%1\">").arg(style.css()) + preserveSpaces(line.toHtmlEscaped()) + "</span>");
        const QString subtle = m_theme.subtle.name();
        const auto corner = [&](const char *name) {
            return QString("<td width=\"6\" height=\"6\" style=\"font-size:1px;line-height:6px;\"><img src=\"%1\" width=\"6\" height=\"6\"></td>")
                .arg(m_theme.resource(QLatin1String(name)));
        };
        const QString edge = QString("<td bgcolor=\"%1\" style=\"font-size:1px;line-height:6px;\">&#8203;</td>").arg(subtle);
        return QString("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:%1px;\">").arg(top)
            + "<tr>" + corner("corner-tl") + edge + corner("corner-tr") + "</tr>"
            + QString("<tr><td bgcolor=\"%1\"></td><td bgcolor=\"%1\" style=\"padding:7px 10px 13px 10px;line-height:17px;\">").arg(subtle)
            + lines.join(QStringLiteral("<br>"))
            + QString("</td><td bgcolor=\"%1\"></td></tr>").arg(subtle)
            + "<tr>" + corner("corner-bl") + edge + corner("corner-br") + "</tr></table>";
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `make -C tray/build -j8 test_markdownhtml && QT_QPA_PLATFORM=offscreen tray/build/tests/test_markdownhtml`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add tray/src/MarkdownHtml.cpp tray/tests/test_markdownhtml.cpp
git commit -m "Render Markdown code blocks, quotes, tables and rules like GitHub"
```

---

### Task 4: Drawn resources, inline-code chips and plain text

**Files:**
- Create: `tray/src/MarkdownObjects.h`, `tray/src/MarkdownObjects.cpp`, `tray/tests/test_markdownobjects.cpp`
- Modify: `tray/cmake/Markdown.cmake` (add the two sources), `tray/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `MarkdownTheme`, `MarkdownHtml::render`, `MarkdownHtml::chipSentinel` (Task 1), list and code-block shapes (Tasks 2–3).
- Produces:
  - `constexpr int MarkdownObjects::ChipObjectType = QTextFormat::UserObject + 41;`
  - `QImage MarkdownObjects::resource(const QUrl &url, qreal devicePixelRatio);`
  - `int MarkdownObjects::convertChips(QTextDocument *document, const MarkdownTheme &theme, bool objects);`
  - `QString MarkdownObjects::plainText(const QTextCursor &selection);`

- [ ] **Step 1: Wire the sources and test**

In `tray/cmake/Markdown.cmake`, extend the `hgs-markdown` sources:

```cmake
add_library(hgs-markdown STATIC
    ${HGS_MARKDOWN_ROOT}/src/MarkdownHtml.h ${HGS_MARKDOWN_ROOT}/src/MarkdownHtml.cpp
    ${HGS_MARKDOWN_ROOT}/src/MarkdownObjects.h ${HGS_MARKDOWN_ROOT}/src/MarkdownObjects.cpp)
```

Append to `tray/tests/CMakeLists.txt`:

```cmake
add_executable(test_markdownobjects test_markdownobjects.cpp)
target_link_libraries(test_markdownobjects PRIVATE Qt6::Test hgs-markdown)
add_test(NAME markdownobjects COMMAND test_markdownobjects)
set_tests_properties(markdownobjects PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
```

- [ ] **Step 2: Write the failing tests**

Create `tray/tests/test_markdownobjects.cpp`:

```cpp
#include "MarkdownHtml.h"
#include "MarkdownObjects.h"

#include <QTest>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextFragment>

namespace {
const MarkdownTheme &light()
{
    static const MarkdownTheme theme = MarkdownTheme::github(false, QColor("#ffffff"));
    return theme;
}
std::unique_ptr<QTextDocument> document(const QString &markdown, const MarkdownLinkPolicy &links = {})
{
    auto result = std::make_unique<QTextDocument>();
    result->setHtml(MarkdownHtml::render(markdown, light(), links));
    return result;
}
QString all(QTextDocument *doc)
{
    QTextCursor cursor(doc); cursor.select(QTextCursor::Document);
    return MarkdownObjects::plainText(cursor);
}
QList<QTextCharFormat> chips(QTextDocument *doc)
{
    QList<QTextCharFormat> result;
    for (auto block = doc->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it)
            if (it.fragment().charFormat().objectType() == MarkdownObjects::ChipObjectType) result.append(it.fragment().charFormat());
    return result;
}
}

class TestMarkdownObjects : public QObject {
    Q_OBJECT
private slots:
    void resourcesAreDrawnAtScaleAndPixelRatio();
    void chipsReplaceSentinelRuns();
    void longCodeStaysWrappableText();
    void searchModeKeepsChipsAsText();
    void chipInsideLinkStaysClickable();
    void plainTextSpellsOutMarkersAndSkipsDecoration();
    void plainTextKeepsBlankLinesInsideMessages();
};

void TestMarkdownObjects::resourcesAreDrawnAtScaleAndPixelRatio()
{
    const auto disc = MarkdownObjects::resource(QUrl("hgs-md:disc/light/150"), 2.0);
    QCOMPARE(disc.size(), QSize(15, 57)); QCOMPARE(disc.devicePixelRatio(), 2.0);
    QVERIFY(qAlpha(disc.pixel(7, 22)) > 200);   // centre of the 5×5 disc at y 5..10
    QVERIFY(qAlpha(disc.pixel(7, 2)) == 0);
    const auto corner = MarkdownObjects::resource(QUrl("hgs-md:corner-br/dark/100"), 1.0);
    QCOMPARE(corner.size(), QSize(6, 6));
    QCOMPARE(QColor::fromRgba(corner.pixel(0, 0)).name(), QString("#151b23"));
    QVERIFY(qAlpha(corner.pixel(5, 5)) < 255);
    for (const char *url : {"hgs-md:disc/blue/100", "hgs-md:unknown/light/100", "hgs-md:disc/light/abc", "https://example.com/x.png", "file:///etc/passwd"})
        QVERIFY2(MarkdownObjects::resource(QUrl(url), 1.0).isNull(), url);
}

void TestMarkdownObjects::chipsReplaceSentinelRuns()
{
    auto doc = document("a `b c` d `e`");
    QCOMPARE(MarkdownObjects::convertChips(doc.get(), light(), true), 2);
    QCOMPARE(chips(doc.get()).size(), 2);
    QCOMPARE(chips(doc.get()).first().property(QTextFormat::UserProperty + 41).toString(), QString("b c"));
    QVERIFY(!doc->toPlainText().contains("b c"));
    QCOMPARE(all(doc.get()), QString("a b c d e"));
}

void TestMarkdownObjects::longCodeStaysWrappableText()
{
    const QString path = "tray/src/a/very/long/path/that/cannot/fit/in/a/narrow/pane.cpp";
    auto doc = document("See `" + path + "`.");
    MarkdownObjects::convertChips(doc.get(), light(), true);
    QVERIFY(chips(doc.get()).isEmpty());
    QVERIFY(doc->toPlainText().contains(path));
}

void TestMarkdownObjects::searchModeKeepsChipsAsText()
{
    auto doc = document("Run `ctest -R x` now");
    QCOMPARE(MarkdownObjects::convertChips(doc.get(), light(), false), 1);
    QVERIFY(chips(doc.get()).isEmpty());
    const auto found = doc->find("ctest -R x");
    QVERIFY(!found.isNull());
    QTextCursor inside(doc.get()); inside.setPosition(found.selectionStart() + 1);
    QCOMPARE(inside.charFormat().background().color(), light().chip);
    QCOMPARE(inside.charFormat().fontFamilies().toStringList().value(0), light().monoFamily);
}

void TestMarkdownObjects::chipInsideLinkStaysClickable()
{
    auto doc = document("[`code`](https://example.com)", [](const QString &href) { return MarkdownLink{href, {}, {}}; });
    MarkdownObjects::convertChips(doc.get(), light(), true);
    QCOMPARE(chips(doc.get()).size(), 1);
    QVERIFY(chips(doc.get()).first().isAnchor());
    QCOMPARE(chips(doc.get()).first().anchorHref(), QString("https://example.com"));
}

void TestMarkdownObjects::plainTextSpellsOutMarkersAndSkipsDecoration()
{
    auto doc = document("Intro with `x`\n\n- a\n- b\n\n3. c\n\n- [x] done\n\n```\nline 1\n  line 2\n```\n\n---\n\nEnd");
    MarkdownObjects::convertChips(doc.get(), light(), true);
    QCOMPARE(all(doc.get()), QString::fromUtf8("Intro with x\n• a\n• b\n3. c\n[x] done\nline 1\n  line 2\nEnd"));
    QVERIFY(!all(doc.get()).contains(QChar::ObjectReplacementCharacter));
}

void TestMarkdownObjects::plainTextKeepsBlankLinesInsideMessages()
{
    // Activity cards are two-column tables; a user's blank line must survive copying.
    QTextDocument doc;
    doc.setHtml("<table><tr><td width='3'></td><td><p style='white-space:pre-wrap;'>first\n\nsecond</p></td></tr></table>");
    QCOMPARE(all(&doc), QString("first\n\nsecond"));
}

QTEST_MAIN(TestMarkdownObjects)
#include "test_markdownobjects.moc"
```

- [ ] **Step 3: Run to verify failure**

Run: `cmake -S tray -B tray/build && make -C tray/build -j8 test_markdownobjects`
Expected: compile error, `MarkdownObjects.h` missing.

- [ ] **Step 4: Implement `MarkdownObjects.h`**

```cpp
#pragma once

#include "MarkdownHtml.h"

#include <QImage>
#include <QTextFormat>
#include <QUrl>

class QTextCursor;
class QTextDocument;

// Pieces of GitHub Markdown that HTML cannot express in Qt rich text.
namespace MarkdownObjects {
constexpr int ChipObjectType = QTextFormat::UserObject + 41;

// Draws hgs-md: resources named by MarkdownTheme::resource(); null for anything else.
QImage resource(const QUrl &url, qreal devicePixelRatio);
// Replaces inline-code sentinel runs by painted chips (GitHub padding and radius).
// With objects=false, or for runs longer than 40 characters, the run stays text
// in chip colours so that find() matches it and narrow panes can wrap it.
int convertChips(QTextDocument *document, const MarkdownTheme &theme, bool objects);
// The selection as a reader sees it: chips give their code, list markers
// "• ", "1. ", "[x] "; code-block corners and rule fillers give nothing.
QString plainText(const QTextCursor &selection);
}
```

- [ ] **Step 5: Implement `MarkdownObjects.cpp`**

```cpp
#include "MarkdownObjects.h"

#include <QAbstractTextDocumentLayout>
#include <QFontMetricsF>
#include <QGuiApplication>
#include <QPainter>
#include <QRegularExpression>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextDocumentFragment>
#include <QTextFragment>
#include <QTextObjectInterface>
#include <QTextTable>

#include <cmath>
#include <functional>

namespace {
enum : int { ChipText = QTextFormat::UserProperty + 41, ChipFill, ChipInk, ChipFamily, ChipScale };
constexpr int MaximumChipLength = 40;

QFont chipFont(const QTextFormat &format)
{
    QFont font(format.stringProperty(ChipFamily));
    font.setPixelSize(qMax(1, qRound(12 * format.doubleProperty(ChipScale))));
    return font;
}

class ChipHandler final : public QObject, public QTextObjectInterface {
    Q_OBJECT
    Q_INTERFACES(QTextObjectInterface)
public:
    using QObject::QObject;
    QSizeF intrinsicSize(QTextDocument *, int, const QTextFormat &format) override
    {
        const QFont font = chipFont(format); const QFontMetricsF metrics(font); const qreal em = font.pixelSize();
        return {metrics.horizontalAdvance(format.stringProperty(ChipText)) + 0.8 * em, metrics.ascent() + metrics.descent() + 0.4 * em};
    }
    void drawObject(QPainter *painter, const QRectF &rect, QTextDocument *, int, const QTextFormat &format) override
    {
        const QFont font = chipFont(format); const QFontMetricsF metrics(font);
        const qreal em = font.pixelSize(), radius = 6 * format.doubleProperty(ChipScale);
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing);
        painter->setPen(Qt::NoPen); painter->setBrush(format.colorProperty(ChipFill));
        painter->drawRoundedRect(rect, radius, radius);
        painter->setFont(font); painter->setPen(format.colorProperty(ChipInk));
        painter->drawText(QPointF(rect.x() + 0.4 * em, rect.y() + 0.2 * em + metrics.ascent()), format.stringProperty(ChipText));
        painter->restore();
    }
};

void install(QTextDocument *document)
{
    auto *layout = document->documentLayout();
    if (layout->findChild<ChipHandler *>()) return;
    layout->registerHandler(MarkdownObjects::ChipObjectType, new ChipHandler(layout));
}

// Qt centres an AlignMiddle object xHeight / 4 above the baseline; pick the
// format font whose x-height puts the chip centre where GitHub has it.
QFont alignmentFont(qreal centreAboveBaseline)
{
    QFont font = QGuiApplication::font();
    for (int pixels = 1; pixels < 400; ++pixels) {
        font.setPixelSize(pixels);
        if (QFontMetricsF(font).xHeight() >= 4 * centreAboveBaseline) break;
    }
    return font;
}

QString markerText(const QString &imageName)
{
    if (!imageName.startsWith(QLatin1String("hgs-md:"))) return QString();
    const QString name = QUrl(imageName).path().section('/', 0, 0);
    if (name == "disc") return QString::fromUtf8("•");
    if (name == "circle") return QString::fromUtf8("◦");
    if (name == "square") return QString::fromUtf8("▪");
    if (name == "check-on") return QStringLiteral("[x]");
    if (name == "check-off") return QStringLiteral("[ ]");
    return QString();
}

QTextTable *tableOf(const QTextBlock &block)
{
    return block.isValid() ? QTextCursor(block).currentTable() : nullptr;
}

bool isMarker(const QString &text)
{
    static const QRegularExpression ordinal(QStringLiteral("^([0-9]+|[ivxlcdm]+|[a-z]+)\\.$"));
    static const QStringList glyphs{QString::fromUtf8("•"), QString::fromUtf8("◦"), QString::fromUtf8("▪"), QStringLiteral("[x]"), QStringLiteral("[ ]")};
    return glyphs.contains(text) || ordinal.match(text).hasMatch();
}
}

QImage MarkdownObjects::resource(const QUrl &url, qreal devicePixelRatio)
{
    if (url.scheme() != QLatin1String("hgs-md")) return {};
    const QStringList parts = url.path().split('/');
    if (parts.size() != 3 || (parts[1] != "light" && parts[1] != "dark")) return {};
    bool ok = false; const double scale = parts[2].toInt(&ok) / 100.0;
    if (!ok || scale < 0.5 || scale > 4.0) return {};
    const bool dark = parts[1] == "dark";
    const auto theme = MarkdownTheme::github(dark, QColor(dark ? "#0d1117" : "#ffffff"), scale);
    const qreal ratio = qMax<qreal>(1.0, devicePixelRatio);
    const auto draw = [&](qreal width, qreal height, const std::function<void(QPainter &)> &paint) {
        QImage image(qCeil(width * scale * ratio), qCeil(height * scale * ratio), QImage::Format_ARGB32_Premultiplied);
        image.setDevicePixelRatio(ratio); image.fill(Qt::transparent);
        QPainter painter(&image); painter.setRenderHint(QPainter::Antialiasing); painter.scale(scale, scale);
        paint(painter);
        return image;
    };
    const QString &name = parts[0];
    // Markers live in a 19 px tall image (rows 5..10) so Qt's baseline alignment matches GitHub.
    if (name == "disc") return draw(5, 19, [&](QPainter &p) { p.setPen(Qt::NoPen); p.setBrush(theme.fg); p.drawEllipse(QRectF(0, 5, 5, 5)); });
    if (name == "circle") return draw(5, 19, [&](QPainter &p) { p.setPen(QPen(theme.fg, 1)); p.setBrush(Qt::NoBrush); p.drawEllipse(QRectF(0.5, 5.5, 4, 4)); });
    if (name == "square") return draw(5, 19, [&](QPainter &p) { p.fillRect(QRectF(0, 5, 5, 5), theme.fg); });
    if (name == "check-on" || name == "check-off") return draw(13, 19, [&](QPainter &p) {
        const bool on = name == "check-on";
        p.setPen(QPen(theme.muted, 1)); p.setBrush(on ? theme.muted : theme.canvas);
        p.drawRoundedRect(QRectF(0.5, 3.5, 12, 12), 2, 2);
        if (on) {
            p.setPen(QPen(theme.canvas, 1.6, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
            p.drawPolyline(QPolygonF{QPointF(3.5, 9.5), QPointF(5.5, 11.5), QPointF(9.5, 7.0)});
        }
    });
    static const QHash<QString, QPointF> corners{{"corner-tl", {0, 0}}, {"corner-tr", {-6, 0}}, {"corner-bl", {0, -6}}, {"corner-br", {-6, -6}}};
    if (corners.contains(name)) return draw(6, 6, [&](QPainter &p) {
        p.setPen(Qt::NoPen); p.setBrush(theme.subtle);
        p.drawRoundedRect(QRectF(corners.value(name), QSizeF(12, 12)), 6, 6);
    });
    return {};
}

int MarkdownObjects::convertChips(QTextDocument *document, const MarkdownTheme &theme, bool objects)
{
    struct Run { int position = 0, length = 0; QString text; QTextCharFormat format; };
    QList<Run> runs;
    for (auto block = document->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto fragment = it.fragment(); const auto format = fragment.charFormat();
            if (format.background().style() == Qt::NoBrush || format.background().color() != MarkdownHtml::chipSentinel()) continue;
            if (!runs.isEmpty() && runs.last().position + runs.last().length == fragment.position()) {
                runs.last().length += fragment.length(); runs.last().text += fragment.text();
            } else runs.append({fragment.position(), fragment.length(), fragment.text(), format});
        }
    if (runs.isEmpty()) return 0;
    const double scale = theme.scalePercent / 100.0;
    if (objects) install(document);
    const QFont anchor = alignmentFont(3.15 * scale);
    for (auto it = runs.crbegin(); it != runs.crend(); ++it) {
        QTextCursor cursor(document);
        cursor.setPosition(it->position); cursor.setPosition(it->position + it->length, QTextCursor::KeepAnchor);
        QString text = it->text; text.replace(QChar::Nbsp, ' ');
        if (!objects || text.size() > MaximumChipLength) {
            // Plain spaces again, so that find() matches and long runs can wrap.
            QTextCharFormat format = it->format;
            format.setBackground(theme.chip); format.setForeground(theme.fg);
            format.setFontFamilies(QStringList{theme.monoFamily});
            format.setProperty(QTextFormat::FontPixelSize, qRound(12 * scale));
            cursor.insertText(text, format);
            continue;
        }
        QTextCharFormat format;
        format.setObjectType(ChipObjectType);
        format.setProperty(ChipText, text); format.setProperty(ChipFill, theme.chip); format.setProperty(ChipInk, theme.fg);
        format.setProperty(ChipFamily, theme.monoFamily); format.setProperty(ChipScale, scale);
        format.setVerticalAlignment(QTextCharFormat::AlignMiddle); format.setFont(anchor);
        if (it->format.isAnchor()) {
            format.setAnchor(true); format.setAnchorHref(it->format.anchorHref()); format.setToolTip(it->format.toolTip());
        }
        cursor.insertText(QString(QChar::ObjectReplacementCharacter), format);
    }
    return runs.size();
}

QString MarkdownObjects::plainText(const QTextCursor &selection)
{
    if (!selection.hasSelection()) return QString();
    QTextDocument copy;
    QTextCursor(&copy).insertFragment(selection.selection());
    QString result;
    for (auto block = copy.begin(); block.isValid(); block = block.next()) {
        QString line;
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto fragment = it.fragment(); const auto format = fragment.charFormat();
            if (format.objectType() == ChipObjectType) {
                for (int i = 0; i < fragment.length(); ++i) line += format.stringProperty(ChipText);
            } else if (format.isImageFormat()) {
                line += markerText(format.toImageFormat().name());
            } else {
                QString text = fragment.text();
                text.remove(QChar(0x200B)).remove(QChar::ObjectReplacementCharacter);
                line += text.replace(QChar::Nbsp, ' ').replace(QChar::LineSeparator, '\n');
            }
        }
        const QTextCursor at(block);
        QTextTable *table = at.currentTable();
        if (line.isEmpty()) {
            // Corners, edges and rule fillers are cells holding one decorative block;
            // Qt also separates adjacent tables with an empty block. Blank lines inside
            // a message (same cell, same table on both sides) are kept.
            bool lone = false;
            if (table) {
                const auto cell = table->cellAt(at);
                lone = cell.firstCursorPosition().block() == block && cell.lastCursorPosition().block() == block;
            }
            if (lone || tableOf(block.previous()) != table || tableOf(block.next()) != table) continue;
        }
        const bool marker = table && table->columns() == 2 && table->cellAt(at).column() == 0 && isMarker(line);
        result += marker ? line + ' ' : line + '\n';
    }
    if (result.endsWith('\n')) result.chop(1);
    return result;
}

#include "MarkdownObjects.moc"
```

- [ ] **Step 6: Run to verify pass**

Run: `make -C tray/build -j8 test_markdownobjects && QT_QPA_PLATFORM=offscreen tray/build/tests/test_markdownobjects`
Expected: all 7 tests PASS. If `plainTextSpellsOutMarkersAndSkipsDecoration` shows an extra blank line, print the block list of `copy` and check that the corresponding cell is inside a `QTextTable`.

- [ ] **Step 7: Commit**

```bash
git add tray/src/MarkdownObjects.h tray/src/MarkdownObjects.cpp tray/tests/test_markdownobjects.cpp \
        tray/cmake/Markdown.cmake tray/tests/CMakeLists.txt
git commit -m "Draw Markdown markers, corners and inline code chips for rich text"
```

---

### Task 5: Use the renderer in the Activity view

**Files:**
- Modify: `tray/src/ActivityView.cpp` (`JournalDocument` 66–71, `markdown()` 192–251, browser creation ~289, `render()` 636–691 and 763/783/831/883)
- Modify: `tray/src/ActivityView.h` (`plainText()`)
- Modify: `tray/tests/CMakeLists.txt` (link `hgs-markdown` into `test_activityview` and `test_sessionswindow`)
- Test: `tray/tests/test_activityview.cpp`

**Interfaces:**
- Consumes: everything from Tasks 1–4.
- Produces: `QString ActivityView::plainText() const` — the document as copied.

- [ ] **Step 1: Link the library into the Activity tests**

In `tray/tests/CMakeLists.txt` change
`target_link_libraries(test_activityview PRIVATE Qt6::Test Qt6::Widgets)` to
`target_link_libraries(test_activityview PRIVATE Qt6::Test Qt6::Widgets hgs-markdown)`
and add `hgs-markdown` to `target_link_libraries(test_sessionswindow PRIVATE ...)`.

- [ ] **Step 2: Write the failing tests and update old expectations**

Add `#include "MarkdownObjects.h"`, `#include <QClipboard>` and `#include <QGuiApplication>` to `tray/tests/test_activityview.cpp`. Add slots `agentMarkdownLooksLikeGitHub`, `searchResultKeepsInlineCodeSearchable`, `copyGivesVisibleText`, `markdownFollowsThemeAndScale`:

```cpp
void TestActivityView::agentMarkdownLooksLikeGitHub()
{
    ActivityView view; view.resize(560, 600); view.show();
    view.setActivity({}, {journalEvent(1, "Stop", "Use `ctest` and:\n\n- one\n- two\n\n```\ncode\n```")});
    auto *document = view.browser()->document();
    int chips = 0; QStringList images;
    for (auto block = document->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            chips += format.objectType() == MarkdownObjects::ChipObjectType;
            if (format.isImageFormat()) images.append(format.toImageFormat().name());
        }
    QCOMPARE(chips, 1);
    QVERIFY(images.contains("hgs-md:disc/light/100")); QVERIFY(images.contains("hgs-md:corner-tl/light/100"));
    for (const auto &name : images) QVERIFY(!document->resource(QTextDocument::ImageResource, QUrl(name)).value<QImage>().isNull());
    QVERIFY(view.plainText().contains(QString::fromUtf8("Use ctest and:\n• one\n• two\ncode")));
}

void TestActivityView::searchResultKeepsInlineCodeSearchable()
{
    ActivityView view; view.resize(560, 400); view.show();
    view.showSearchResult(journalEvent(1, "AgentMessage", "Run `ctest -R markdown` now"), "ctest");
    for (auto block = view.browser()->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(it.fragment().charFormat().objectType() != MarkdownObjects::ChipObjectType);
    QVERIFY(!view.browser()->extraSelections().isEmpty());
}

void TestActivityView::copyGivesVisibleText()
{
    ActivityView view; view.resize(560, 400); view.show();
    view.setActivity({}, {journalEvent(1, "Stop", "Edit `src/cli.rs`:\n\n1. first\n2. second")});
    view.browser()->selectAll(); view.browser()->copy();
    const auto copied = QGuiApplication::clipboard()->text();
    QVERIFY2(copied.contains("Edit src/cli.rs:\n1. first\n2. second"), qPrintable(copied));
    QVERIFY(!copied.contains(QChar::ObjectReplacementCharacter));
}

void TestActivityView::markdownFollowsThemeAndScale()
{
    ActivityView view; view.resize(560, 400); view.show();
    view.setActivity({}, {journalEvent(1, "Stop", "- item with `code`")});
    view.setTheme(true); view.setContentScale(2.0);
    QStringList images; QList<QTextCharFormat> chips;
    for (auto block = view.browser()->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            if (format.isImageFormat()) images.append(format.toImageFormat().name());
            if (format.objectType() == MarkdownObjects::ChipObjectType) chips.append(format);
        }
    QVERIFY(images.contains("hgs-md:disc/dark/200"));
    QCOMPARE(chips.size(), 1);
    QCOMPARE(chips.first().property(QTextFormat::UserProperty + 45).toDouble(), 2.0);
    const auto disc = view.browser()->document()->resource(QTextDocument::ImageResource, QUrl("hgs-md:disc/dark/200")).value<QImage>();
    QCOMPARE(disc.size(), (QSizeF(10, 38) * view.browser()->devicePixelRatioF()).toSize());
}
```

Update existing expectations:

- In `rejectsMarkupResourcesAndUnsafeLinks` replace
  `for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(!it.fragment().charFormat().isImageFormat());` with
  `for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(!it.fragment().charFormat().isImageFormat() || it.fragment().charFormat().toImageFormat().name().startsWith("hgs-md:"));`
  (the agent reply contains a code block; the user-message tests keep the strict check).
- In `preservesLiteralUserMessages` replace `QVERIFY(browser->toPlainText().contains("Ответ агента с кодом"));` with `QVERIFY(view.plainText().contains("Ответ агента с кодом"));`.
- In `contentScaleEnlargesTranscriptAndQueue` the agent body is now 14 px: `QCOMPARE(body, 13);` → `QCOMPARE(body, 14);` and both `QCOMPARE(fontPixels(browser, "Agent body"), 26);` → `QCOMPARE(fontPixels(browser, "Agent body"), 28);`.

- [ ] **Step 3: Run to verify failure**

Run: `make -C tray/build -j8 test_activityview && QT_QPA_PLATFORM=offscreen tray/build/tests/test_activityview`
Expected: compile error (`plainText` is not a member of `ActivityView`); after adding only the declaration, the four new tests and the updated ones FAIL.

- [ ] **Step 4: Implement the integration**

`tray/src/ActivityView.h`, after `bool replyVisible(...)`:

```cpp
    // The document as copied: chips and list markers spelled out.
    QString plainText() const;
```

`tray/src/ActivityView.cpp`:

1. Add includes: `#include "MarkdownHtml.h"`, `#include "MarkdownObjects.h"`, `#include <QMimeData>`.
2. Replace `JournalDocument` with:

```cpp
class JournalDocument final : public QTextDocument {
public:
    using QTextDocument::QTextDocument;
    qreal pixelRatio = 1.0;
protected:
    // Only drawn Markdown decorations load; remote and file resources never do.
    QVariant loadResource(int, const QUrl &url) override { return MarkdownObjects::resource(url, pixelRatio); }
};

// Copies what the reader sees instead of U+FFFC for chips and markers.
class ActivityBrowser final : public QTextBrowser {
public:
    using QTextBrowser::QTextBrowser;
protected:
    QMimeData *createMimeDataFromSelection() const override
    {
        auto *data = new QMimeData;
        data->setText(MarkdownObjects::plainText(textCursor()));
        return data;
    }
};
```

3. Replace the whole `markdown()` function (it starts at `QString markdown(const QString &text, const QString &codeBackground, ...`) with:

```cpp
QString markdown(const QString &text, const MarkdownTheme &theme, QHash<QString, QString> &fileLinks)
{
    return MarkdownHtml::render(text, theme, [&fileLinks](const QString &href) -> MarkdownLink {
        const auto file = SessionFileReference::parse(href);
        if (file.valid()) {
            const QString key = QString::fromLatin1(QCryptographicHash::hash(href.toUtf8(), QCryptographicHash::Sha256).toHex());
            fileLinks.insert(key, href);
            return {"hgs-file:" + key, file.path + file.location, file.path + file.location};
        }
        if (webLink(QUrl(href))) return {href, {}, {}};
        return {};
    });
}
```

4. In the constructor, `m_browser = new QTextBrowser(this);` → `m_browser = new ActivityBrowser(this);`.
5. In `render()`, after the colour constants (`const QString border = ...`), add:

```cpp
    // Agent replies sit on GitHub's canvas; expanded thinking stays on the page.
    const auto agentTheme = MarkdownTheme::github(m_dark, QColor(m_dark ? "#0d1117" : "#ffffff"), m_scale);
    const auto pageTheme = MarkdownTheme::github(m_dark, QColor(m_dark ? "#1c2229" : "#ffffff"), m_scale);
```

6. In `card`, change `user ? userSurface : surface` to `user ? userSurface : agentTheme.canvas.name()`.
7. Replace the three calls:
   - `markdown(answer, codeSurface, accent, m_fileLinks)` → `markdown(answer, agentTheme, m_fileLinks)`
   - `: markdown(text, codeSurface, accent, m_fileLinks);` → `: markdown(text, agentTheme, m_fileLinks);`
   - `markdown(event.value("detail").toString(),codeSurface,accent,m_fileLinks)` → `markdown(event.value("detail").toString(),pageTheme,m_fileLinks)`
8. Replace `m_browser->setHtml(html);` with:

```cpp
    static_cast<JournalDocument *>(m_browser->document())->pixelRatio = m_browser->devicePixelRatioF();
    m_browser->setHtml(html);
    // Before bookmarks are read: offsets on both sides of a refresh count chips as one character.
    MarkdownObjects::convertChips(m_browser->document(), agentTheme, !searching);
```

9. Add the accessor after `replyVisible()`:

```cpp
QString ActivityView::plainText() const
{
    QTextCursor all(m_browser->document());
    all.select(QTextCursor::Document);
    return MarkdownObjects::plainText(all);
}
```

10. Remove the `QTextDocument::Markdown...` usage (it disappears with the old `markdown()`); keep `webLink()`.

In `tray/CMakeLists.txt` `hgs-markdown` is already linked into `hgs-tray` (Task 1).

- [ ] **Step 5: Run the Activity and window suites**

Run: `make -C tray/build -j8 test_activityview test_sessionswindow && QT_QPA_PLATFORM=offscreen tray/build/tests/test_activityview && QT_QPA_PLATFORM=offscreen tray/build/tests/test_sessionswindow`
Expected: all PASS. If another existing test reads agent Markdown through `browser->toPlainText()` and now sees `U+FFFC` or a marker on its own line, change that assertion to `view.plainText()`; do not change user-message assertions.

- [ ] **Step 6: Commit**

```bash
git add tray/src/ActivityView.h tray/src/ActivityView.cpp tray/tests/test_activityview.cpp tray/tests/CMakeLists.txt
git commit -m "Show agent replies with GitHub Markdown styling in Activity"
```

---

### Task 6: Visual check and full verification

**Files:**
- Modify: `tray/tests/test_activityview.cpp` (`preview()` sample)

- [ ] **Step 1: Extend the preview sample**

In `TestActivityView::preview()`, replace the `journalEvent(6, "Stop", ...)` text with:

```cpp
            journalEvent(6, "Stop", "## Summary\n\nFixed launching sessions with a space in the name:\n\n"
                "- `src/cli.rs` — `--new` now follows `rename`\n- `tests/test_hgs.sh` — new checks\n  - nested item with **bold** text\n"
                "- see [docs](https://example.com) and [client registry](docs/client.md:24)\n\n1. First step\n2. Second step\n\n"
                "| File | Lines | Status |\n|------|------:|--------|\n| `src/cli.rs` | 7 | changed |\n| `tests/test_hgs.sh` | 18 | added |\n| `README.md` | 0 | unchanged |\n\n"
                "> Remote machines must update `hgs`.\n\n```sh\nctest --test-dir tray/build -R activityview\n```\n\n---\n\nAll focused checks passed.")});
```

- [ ] **Step 2: Render and inspect both themes**

Run:

```bash
make -C tray/build -j8 test_activityview
out=$(mktemp -d); HGS_PREVIEW_DIR=$out QT_QPA_PLATFORM=offscreen tray/build/tests/test_activityview preview; ls "$out"
```

Open `activity-light.png` and `activity-dark.png`. Compare with a GitHub comment showing the same Markdown (or the prototype reference renders, if still available). Check: compact list rhythm with round markers 11 px left of the text; inline code as rounded chips; table with 1 px borders, bold centred header and zebra rows; quote with a 4 px bar and muted text; code block with rounded corners and 12 px monospace; 4 px rule; agent card on `#ffffff` / `#0d1117`. Fix any deviation in `MarkdownHtml.cpp`/`MarkdownObjects.cpp` with a test, then re-render.

- [ ] **Step 3: Full verification**

Run:

```bash
make -C tray/build -j8
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu ctest --test-dir tray/build --parallel 1 --timeout 240 --output-on-failure
python3 scripts/check-source-privacy.py
```

Expected: 100 % of Qt tests pass (now including `markdownhtml` and `markdownobjects`); the privacy check reports only the pre-existing `.beads/issues.jsonl` finding.

- [ ] **Step 4: Close the tracker item and commit**

```bash
bd close zerus-0bg --reason="Agent Markdown renders like GitHub in Activity"
git add tray/tests/test_activityview.cpp
git commit -m "Preview the GitHub Markdown styling in Activity snapshots"
```

The running GUI picks the change up only after the user restarts it; do not restart it.
