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
