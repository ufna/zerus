#include "ContentScale.h"
#include "MarkdownHtml.h"
#include "MarkdownObjects.h"

#include <QAbstractTextDocumentLayout>
#include <QElapsedTimer>
#include <QFontMetricsF>
#include <QImage>
#include <QPainter>
#include <QPalette>
#include <QTest>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextFragment>
#include <QTextLayout>

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
// The Activity view's drawing: the document on white, then inline code that stayed text.
QImage rendered(QTextDocument *doc, const QList<QAbstractTextDocumentLayout::Selection> &selections = {})
{
    auto *layout = doc->documentLayout();
    QImage image(layout->documentSize().toSize(), QImage::Format_ARGB32_Premultiplied); image.fill(Qt::white);
    QPalette palette; palette.setColor(QPalette::Base, Qt::white);
    QAbstractTextDocumentLayout::PaintContext context; context.palette = palette; context.selections = selections;
    QPainter painter(&image);
    layout->draw(&painter, context);
    QList<QTextCursor> cursors;
    for (const auto &selection : selections) cursors.append(selection.cursor);
    MarkdownObjects::paintTextChips(&painter, doc, image.rect(), palette, cursors);
    return image;
}
// Equal up to the step or two that antialiasing and compositing round away.
bool close(const QColor &a, const QColor &b)
{
    return qAbs(a.red() - b.red()) <= 3 && qAbs(a.green() - b.green()) <= 3 && qAbs(a.blue() - b.blue()) <= 3;
}
QColor over(const QColor &top, const QColor &bottom)
{
    const auto mix = [&](int a, int b) { return qRound(a * top.alphaF() + b * (1 - top.alphaF())); };
    return QColor(mix(top.red(), bottom.red()), mix(top.green(), bottom.green()), mix(top.blue(), bottom.blue()));
}
// Where a 24 px code run of a scale-2 paragraph sits, in document pixels.
struct CodeGeometry {
    QTextBlock block; QPointF origin; QFontMetricsF metrics{QFont()}; int start = 0, end = 0;
    CodeGeometry(QTextDocument *doc, const QString &code, const QString &family) : block(doc->begin())
    {
        QFont mono(family); mono.setPixelSize(24); metrics = QFontMetricsF(mono);
        origin = doc->documentLayout()->blockBoundingRect(block).topLeft();
        start = block.text().indexOf(code); end = start + code.size();
    }
    QTextLine line(int position) const { return block.layout()->lineForTextPosition(position); }
    qreal x(int position) const { return origin.x() + line(position).cursorToX(position); }
    qreal baseline(const QTextLine &l) const { return origin.y() + l.y() + l.ascent(); }
    qreal top(const QTextLine &l) const { return baseline(l) - metrics.ascent() - 0.2 * 24; }
    qreal bottom(const QTextLine &l) const { return baseline(l) + metrics.descent() + 0.2 * 24; }
};
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
    void chipsFollowTheirTextSizeAndColour();
    void longJournalsConvertQuickly();
    void narrowPanesKeepWideCodeAsText();
    void chipsShareTheTextBaseline();
    void chipLinesKeepTheParagraphPitch();
    void wrappedCodeKeepsRoundedEnds();
    void selectedCodeKeepsTheSelectionLook();
};

void TestMarkdownObjects::resourcesAreDrawnAtScaleAndPixelRatio()
{
    const auto disc = MarkdownObjects::resource(QUrl("hgs-md:disc/light/150"), 2.0);
    QCOMPARE(disc.size(), QSize(84, 36)); QCOMPARE(disc.devicePixelRatio(), 2.0);
    QVERIFY(qAlpha(disc.pixel(40, 19)) > 200);   // centre of the 5×5 disc at x 11..16, y 4..9
    QVERIFY(qAlpha(disc.pixel(7, 2)) == 0);
    const auto corner = MarkdownObjects::resource(QUrl("hgs-md:corner-br/dark/100"), 1.0);
    QCOMPARE(corner.size(), QSize(6, 6));
    QCOMPARE(QColor::fromRgba(corner.pixel(0, 0)).name(), QString("#151b23"));
    QVERIFY(qAlpha(corner.pixel(5, 5)) < 255);
    const auto filled = MarkdownObjects::resource(QUrl("hgs-md:corner-tl/light/100?fill=e8eef2"), 1.0);
    QCOMPARE(QColor::fromRgba(filled.pixel(5, 5)).name(), QString("#e8eef2"));
    for (const char *url : {"hgs-md:corner-tl/light/100?fill=zz0000", "hgs-md:disc/blue/100", "hgs-md:unknown/light/100", "hgs-md:disc/light/abc", "https://example.com/x.png", "file:///etc/passwd"})
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
    // paintTextChips() draws the rounded chip: a character background would be square.
    // GitHub pads code .4em on both sides: the character before it and its last one advance further.
    const int start = doc->toPlainText().indexOf(path);
    QTextCursor at(doc.get());
    for (const int after : {start, start + int(path.size())}) {
        at.setPosition(after);
        QCOMPARE(at.charFormat().fontLetterSpacingType(), QFont::AbsoluteSpacing);
        QCOMPARE(at.charFormat().fontLetterSpacing(), 0.4 * 12);
    }
    at.setPosition(start + 1);
    QCOMPARE(at.charFormat().fontLetterSpacing(), 0.0);
    QCOMPARE(at.charFormat().background().style(), Qt::NoBrush);
}

void TestMarkdownObjects::searchModeKeepsChipsAsText()
{
    auto doc = document("Run `ctest -R x` now");
    QCOMPARE(MarkdownObjects::convertChips(doc.get(), light(), false), 1);
    QVERIFY(chips(doc.get()).isEmpty());
    const auto found = doc->find("ctest -R x");
    QVERIFY(!found.isNull());
    QTextCursor inside(doc.get()); inside.setPosition(found.selectionStart() + 1);
    QCOMPARE(inside.charFormat().colorProperty(QTextFormat::UserProperty + 42), light().chip);
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

void TestMarkdownObjects::chipsFollowTheirTextSizeAndColour()
{
    auto doc = document("## Title `main`\n\nBody `main`\n\n###### Note `main`");
    QCOMPARE(MarkdownObjects::convertChips(doc.get(), light(), true), 3);
    const auto found = chips(doc.get());
    QCOMPARE(found.size(), 3);
    QCOMPARE(found[0].intProperty(QTextFormat::UserProperty + 46), 21);   // heading size, not 85 %
    QVERIFY(found[0].boolProperty(QTextFormat::UserProperty + 47));         // heading padding 0 .2em
    QCOMPARE(found[0].intProperty(QTextFormat::UserProperty + 48), 600);    // heading weight
    QCOMPARE(found[1].intProperty(QTextFormat::UserProperty + 46), 12);
    QVERIFY(!found[1].boolProperty(QTextFormat::UserProperty + 47));
    QCOMPARE(found[2].colorProperty(QTextFormat::UserProperty + 43).name(), QString("#59636e"));
    // The heading chip sits higher, as its code baseline follows the larger text.
    QVERIFY(found[0].font().pixelSize() > found[1].font().pixelSize());
    auto search = document("## Title `main`");
    MarkdownObjects::convertChips(search.get(), light(), false);
    QTextCursor inside(search.get()); inside.setPosition(search->find("main").selectionStart() + 1);
    QCOMPARE(inside.charFormat().font().pixelSize(), 21);
}

void TestMarkdownObjects::longJournalsConvertQuickly()
{
    // Activity re-renders on every poll; 4000 chips in 800 card tables must not stall the GUI thread.
    QString html;
    for (int i = 0; i < 800; ++i)
        html += "<table width='100%'><tr><td width='3'></td><td>"
            + MarkdownHtml::render(QString("Reply %1 uses `a` `b` `c` `d` `e`.").arg(i), light(), {}) + "</td></tr></table>";
    for (bool objects : {true, false}) {
        auto doc = std::make_unique<QTextDocument>(); doc->setHtml(html);
        doc->documentLayout()->documentSize();
        QElapsedTimer timer; timer.start();
        QCOMPARE(MarkdownObjects::convertChips(doc.get(), light(), objects), 4000);
        QVERIFY2(timer.elapsed() < 1000, qPrintable(QString::number(timer.elapsed()) + " ms"));
    }
}

void TestMarkdownObjects::narrowPanesKeepWideCodeAsText()
{
    // A chip cannot wrap: one wider than the space it may take stays wrappable text.
    auto narrow = document("Run `cargo test --workspace` now");
    QCOMPARE(MarkdownObjects::convertChips(narrow.get(), light(), true, 60), 1);
    QVERIFY(chips(narrow.get()).isEmpty());
    QVERIFY(narrow->toPlainText().contains("cargo test --workspace"));
    auto wide = document("Run `cargo test --workspace` now");
    MarkdownObjects::convertChips(wide.get(), light(), true, 600);
    QCOMPARE(chips(wide.get()).size(), 1);
}

void TestMarkdownObjects::chipsShareTheTextBaseline()
{
    // GitHub pads code symmetrically around text on the line's baseline: below
    // it a lone chip reaches the code descent plus the .2em padding.
    auto doc = document("`code`");
    MarkdownObjects::convertChips(doc.get(), light(), true);
    doc->documentLayout()->documentSize();
    QFont code(light().monoFamily); code.setPixelSize(12);
    const qreal below = QFontMetricsF(code).descent() + 0.2 * 12;
    const QTextLine line = doc->begin().layout()->lineAt(0);
    QVERIFY2(qAbs(line.descent() - below) <= 0.5, qPrintable(QString("%1 px instead of %2 px").arg(line.descent()).arg(below)));
}

void TestMarkdownObjects::chipLinesKeepTheParagraphPitch()
{
    // A chip reaching further below the baseline than text must neither grow its
    // line nor lift its baseline: every line of a paragraph keeps the same pitch.
    const QString markdown = "Only the round hash decides. `setWinner` / `#updateWinner` and the old bot are gone. "
        "Every round on the stand matched its hash, which plain lines like this one and the next one show without "
        "any code at all, until `roll` comes back at the very end.";
    for (const double scale : {1.0, 1.25}) {
        const auto theme = MarkdownTheme::github(false, QColor("#ffffff"), scale);
        QTextDocument doc; doc.setTextWidth(qRound(260 * scale));
        doc.setHtml(ContentScale::html(MarkdownHtml::render(markdown, theme, {}), scale));
        QCOMPARE(MarkdownObjects::convertChips(&doc, theme, true), 3);
        doc.documentLayout()->documentSize();
        const QTextBlock block = doc.begin();
        const QTextLayout *layout = block.layout();
        int withChips = 0;
        for (int i = 0; i < layout->lineCount(); ++i) {
            const QTextLine line = layout->lineAt(i);
            withChips += block.text().mid(line.textStart(), line.textLength()).contains(QChar::ObjectReplacementCharacter);
            if (i == 0) continue;
            const QTextLine previous = layout->lineAt(i - 1);
            QCOMPARE(line.y() + line.ascent() - previous.y() - previous.ascent(), block.blockFormat().lineHeight());
        }
        QVERIFY2(withChips > 0 && withChips < layout->lineCount(), "the paragraph needs lines with and without chips");
    }
}

void TestMarkdownObjects::wrappedCodeKeepsRoundedEnds()
{
    // Code that stays text wraps like GitHub's (box-decoration-break: slice): one
    // chip, rounded and padded where the code begins and ends, cut square where a
    // line breaks it. The spaces hanging at a break stay outside.
    const auto theme = MarkdownTheme::github(false, QColor("#ffffff"), 2.0);
    const QString code = "export HGS_STATE_DIR HGS_CONFIG_DIR HGS_RUNTIME_DIR HGS_LOG_DIR";
    QTextDocument doc; doc.setTextWidth(560);
    doc.setHtml(ContentScale::html(MarkdownHtml::render("Then run `" + code + "` again.", theme, {}), 2.0));
    MarkdownObjects::convertChips(&doc, theme, true);
    const QImage image = rendered(&doc);
    const CodeGeometry g(&doc, code, theme.monoFamily);
    const QTextLine first = g.line(g.start), last = g.line(g.end - 1);
    QVERIFY2(last.lineNumber() - first.lineNumber() == 2, "the code must take three lines");
    const QTextLine middle = g.block.layout()->lineAt(first.lineNumber() + 1);
    const auto pixel = [&](qreal x, qreal y) { return image.pixelColor(qFloor(x), qFloor(y)); };
    const QColor fill = over(theme.chip, Qt::white);
    const qreal padding = 0.4 * 24, radius = 12;
    const qreal left = g.x(g.start) - padding, right = g.x(g.end);
    QVERIFY(close(pixel(left + 1, g.top(first) + 1), Qt::white));
    QVERIFY(close(pixel(left + radius, g.top(first) + 1), fill));
    QVERIFY(close(pixel(left + padding / 2, (g.top(first) + g.bottom(first)) / 2), fill));
    QVERIFY(close(pixel(right - 1, g.bottom(last) - 1), Qt::white));
    QVERIFY(close(pixel(right - padding / 2, (g.top(last) + g.bottom(last)) / 2), fill));
    for (const QTextLine &line : {first, middle}) {
        const int broken = line.textStart() + line.textLength();
        QVERIFY(g.block.text().at(broken - 1).isSpace());
        const qreal cut = g.origin.x() + line.cursorToX(broken - 1);
        QVERIFY(close(pixel(cut - 1, g.top(line) + 1), fill));
        QVERIFY(close(pixel(cut + 2, (g.top(line) + g.bottom(line)) / 2), Qt::white));
    }
    for (const QTextLine &line : {middle, last})
        QVERIFY(close(pixel(g.origin.x() + line.cursorToX(line.textStart()) + 1, g.bottom(line) - 1), fill));
    // The glyphs lie on the chip, untinted.
    bool ink = false;
    for (int y = qCeil(g.top(first)); y < g.bottom(first); ++y)
        for (int x = qCeil(left); x < g.origin.x() + first.naturalTextWidth(); ++x) ink = ink || close(image.pixelColor(x, y), theme.fg);
    QVERIFY(ink);
}

void TestMarkdownObjects::selectedCodeKeepsTheSelectionLook()
{
    // Selected and found characters show the view's highlight, not the chip.
    const auto theme = MarkdownTheme::github(false, QColor("#ffffff"), 2.0);
    const QString code = "cargo test --workspace --all-features --no-fail-fast";
    QTextDocument doc; doc.setTextWidth(1600);
    doc.setHtml(ContentScale::html(MarkdownHtml::render("Then run `" + code + "` again.", theme, {}), 2.0));
    MarkdownObjects::convertChips(&doc, theme, true);
    const CodeGeometry g(&doc, code, theme.monoFamily);
    QTextCursor selected(&doc);
    selected.setPosition(g.block.position() + g.start + 6); selected.setPosition(g.block.position() + g.start + 13, QTextCursor::KeepAnchor);
    QTextCharFormat highlight; highlight.setBackground(QColor("#00ff00"));
    const QImage image = rendered(&doc, {{selected, highlight}});
    // The spaces after "cargo" and after "test", halfway up the code.
    const auto middleOf = [&](int offset) {
        const QTextLine line = g.line(g.start + offset);
        return image.pixelColor(qFloor((g.x(g.start + offset) + g.x(g.start + offset + 1)) / 2), qFloor(g.baseline(line) - g.metrics.ascent() / 2));
    };
    QVERIFY(close(middleOf(5), over(theme.chip, Qt::white)));
    QVERIFY(close(middleOf(10), QColor("#00ff00")));
}

QTEST_MAIN(TestMarkdownObjects)
#include "test_markdownobjects.moc"
