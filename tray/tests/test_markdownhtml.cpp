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
// Linked text and its destination, one entry per link.
QStringList linked(const QTextDocument &doc)
{
    QList<QPair<QString, QString>> links; QString previous;
    for (auto block = doc.begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            const QString href = format.isAnchor() ? format.anchorHref() : QString();
            if (!href.isEmpty() && href == previous) links.last().first += it.fragment().text();
            else if (!href.isEmpty()) links.append({it.fragment().text(), href});
            previous = href;
        }
    QStringList result;
    for (const auto &[text, href] : links) result.append(text + " -> " + href);
    return result;
}
MarkdownLink policy(const QString &destination)
{
    if (destination.startsWith("https://") || destination.startsWith("http://")) return {destination, {}, {}};
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
    void bareUrlsLinkLikeGitHub_data();
    void bareUrlsLinkLikeGitHub();
    void codeUrlsLookLikeCodeLinks();
    void imagesBecomePlaceholders();
    void entitiesAreDecoded();
    void inlineCodeUsesSentinelAndTallerLine();
    void monospaceFamilyIsInstalled();
    void bulletListsUseDrawnMarkers();
    void orderedListsHonourStartAndNesting();
    void looseListItemsAreSpaced();
    void taskItemsShowCheckboxes();
    void deepNestingSettles();
    void codeBlocksKeepWhitespaceAndRoundCorners();
    void codeBlocksIgnoreCarriageReturns();
    void blockquotesHaveBarAndMutedText();
    void tablesHaveBordersZebraAndAlignment();
    void rulesUseGitHubGaps();
    void markerSharesTheLineWithItsText();
    void inlineCodeFollowsSurroundingText();
    void listMarkersStandOnTheBaseline();
    void tableStripesUseTheirOwnToken();
};

void TestMarkdownHtml::themeUsesGitHubTokens()
{
    const auto dark = MarkdownTheme::github(true, QColor("#0d1117"), 1.5);
    QCOMPARE(light().fg.name(), QString("#1f2328")); QCOMPARE(dark.fg.name(), QString("#f0f6fc"));
    QCOMPARE(light().subtle.name(), QString("#f6f8fa")); QCOMPARE(dark.subtle.name(), QString("#151b23"));
    QVERIFY(qAbs(light().chip.alphaF() - 0.12) < 0.001); QVERIFY(qAbs(dark.chip.alphaF() - 0.2) < 0.001);
    QCOMPARE(dark.resource("disc"), QString("hgs-md:disc/dark/150"));
    QCOMPARE(light().resource("corner-tl"), QString("hgs-md:corner-tl/light/100?fill=f6f8fa"));
    // The embedding view may put code on its own surface; corners follow it.
    auto card = light(); card.subtle = QColor("#e8eef2");
    QCOMPARE(card.resource("corner-br"), QString("hgs-md:corner-br/light/100?fill=e8eef2"));
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

void TestMarkdownHtml::bareUrlsLinkLikeGitHub_data()
{
    // GitHub's extended autolinks: a scheme URL needs no dot in its host, and the
    // link runs to whitespace without trailing punctuation or unbalanced ")".
    QTest::addColumn<QString>("markdown");
    QTest::addColumn<QStringList>("links");
    QTest::newRow("local server") << "Open http://localhost:3060/ now"
        << QStringList{"http://localhost:3060/ -> http://localhost:3060/"};
    QTest::newRow("fragments") << "See http://localhost:3060/#a (Hills), http://127.0.0.1:8765/#settings."
        << QStringList{"http://localhost:3060/#a -> http://localhost:3060/#a", "http://127.0.0.1:8765/#settings -> http://127.0.0.1:8765/#settings"};
    QTest::newRow("strong") << "Open **http://localhost:3060/**."
        << QStringList{"http://localhost:3060/ -> http://localhost:3060/"};
    QTest::newRow("port and query") << "https://example.com:8443/path?x=1&y=2!"
        << QStringList{"https://example.com:8443/path?x=1&y=2 -> https://example.com:8443/path?x=1&y=2"};
    QTest::newRow("parentheses") << "(see https://example.com/a_(b))"
        << QStringList{"https://example.com/a_(b) -> https://example.com/a_(b)"};
    QTest::newRow("unicode path") << "https://example.com/путь, done"
        << QStringList{"https://example.com/путь -> https://example.com/путь"};
    QTest::newRow("www") << "Visit www.example.com." << QStringList{"www.example.com -> http://www.example.com"};
    QTest::newRow("table cell") << "| Site |\n|---|\n| http://localhost:3050/ |"
        << QStringList{"http://localhost:3050/ -> http://localhost:3050/"};
    QTest::newRow("explicit link") << "[label](https://example.com/x) https://example.org"
        << QStringList{"label -> https://example.com/x", "https://example.org -> https://example.org"};
    QTest::newRow("inline code") << "Open `http://localhost:3060/#a` or `www.example.com`."
        << QStringList{"http://localhost:3060/#a -> http://localhost:3060/#a", "www.example.com -> http://www.example.com"};
    QTest::newRow("code inside a link") << "[`http://localhost:3060/`](https://example.com/x)"
        << QStringList{"http://localhost:3060/ -> https://example.com/x"};
    QTest::newRow("command") << "`curl http://localhost:3060/` and `http://localhost:3060/.`" << QStringList{};
    QTest::newRow("code block") << "```\nhttp://localhost:3060/\n```" << QStringList{};
    QTest::newRow("inside a word") << "xhttp://localhost:3060/ awww.example.com" << QStringList{};
    QTest::newRow("no host") << "http:// and https://?x" << QStringList{};
    QTest::newRow("policy") << "ftp://example.com/file" << QStringList{};
}

void TestMarkdownHtml::bareUrlsLinkLikeGitHub()
{
    QFETCH(QString, markdown); QFETCH(QStringList, links);
    QCOMPARE(linked(*document(markdown, policy)), links);
}

void TestMarkdownHtml::codeUrlsLookLikeCodeLinks()
{
    // Agents often quote an address as code. The whole span is then a link, as if
    // written [`url`](url); only an exact URL qualifies, so commands stay literal.
    QCOMPARE(html("Open `http://localhost:3060/#a` now", policy),
        html("Open [`http://localhost:3060/#a`](http://localhost:3060/#a) now", policy));
    QCOMPARE(html("## `https://example.com/`", policy), html("## [`https://example.com/`](https://example.com/)", policy));
    QVERIFY(!html("`http://localhost:3060/`", [](const QString &) { return MarkdownLink{}; }).contains("<a "));
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

void TestMarkdownHtml::bulletListsUseDrawnMarkers()
{
    const auto output = html("Intro\n\n- a\n- b\n  - c\n    - d");
    QVERIFY(output.contains(light().resource("disc"))); QVERIFY(output.contains(light().resource("circle")));
    QVERIFY(output.contains(light().resource("square")));
    QVERIFY(output.contains("<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:16px;\">"));
    QVERIFY(output.contains("<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:0px;\">"));
    QVERIFY(output.contains("<td style=\"padding:0px 0 0 28px;\">"));
    QVERIFY(output.contains("<td style=\"padding:3px 0 0 28px;\">"));
    QVERIFY(output.contains("<p style=\"margin:0px 0 0 0;line-height:21px;text-indent:-28px;\">"));
    QVERIFY(document("- a\n- b")->toPlainText().contains("a"));
}

void TestMarkdownHtml::orderedListsHonourStartAndNesting()
{
    const auto plain = document("3. a\n4. b\n   1. c\n      1. d")->toPlainText();
    for (const char *marker : {"3.", "4.", "i.", "a."}) QVERIFY2(plain.contains(QLatin1String(marker)), marker);
    QVERIFY(html("1. a\n2. b").contains(light().resource("gap")));
    QVERIFY(html("1. a\n2. b").contains(light().resource("blank")));
}

void TestMarkdownHtml::looseListItemsAreSpaced()
{
    QVERIFY(html("- a\n\n- b").contains("padding:16px 0 0 28px;"));
    QVERIFY(html("- a\n\n- b").contains("<p style=\"margin:0px 0 0 0;line-height:21px;text-indent:-28px;\">"));
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

void TestMarkdownHtml::markerSharesTheLineWithItsText()
{
    // In a cell of its own the marker follows a different baseline than a first
    // line set in monospace or holding a chip; on the same line it cannot drift.
    const auto doc = document("- `tray/src/a/very/long/path/that/cannot/fit/in/a/narrow/pane.cpp`\n- short `x` item\n- plain item\n"
                              "- [ ] task item\n\n1. numbered item");
    for (const char *text : {"tray/src/a/very", "short", "plain item", "task item", "numbered item"}) {
        bool found = false;
        for (auto block = doc->begin(); block.isValid(); block = block.next()) {
            if (!block.text().contains(QLatin1String(text))) continue;
            bool marker = false;
            for (auto it = block.begin(); !it.atEnd(); ++it)
                marker |= it.fragment().charFormat().isImageFormat() && it.fragment().charFormat().toImageFormat().name().startsWith("hgs-md:");
            QVERIFY2(marker, text); found = true;
        }
        QVERIFY2(found, text);
    }
    QVERIFY(html("- ```\n  x\n  ```").contains(light().resource("disc")));
}

void TestMarkdownHtml::inlineCodeFollowsSurroundingText()
{
    // GitHub: code is 85 % of the text around it and inherits its colour and weight;
    // in headings it keeps the heading size (`h2 code { font-size: inherit }`).
    QVERIFY(html("Run `x` now").contains("background-color:#010203;font-size:12px;color:#1f2328;\""));
    QVERIFY(html("**Bold `x`**").contains("background-color:#010203;font-size:12px;color:#1f2328;font-weight:600;"));
    QVERIFY(html("## Title `main` here").contains("background-color:#010204;font-size:21px;color:#1f2328;font-weight:600;"));
    QVERIFY(html("# Big `main`").contains("background-color:#010204;font-size:28px;"));
    QVERIFY(html("###### Small `main`").contains("background-color:#010204;font-size:12px;color:#59636e;"));
    QVERIFY(html("[`code`](https://example.com)", policy).contains("font-size:12px;color:#0969da;"));
    QCOMPARE(MarkdownHtml::headingChipSentinel(), QColor(1, 2, 4));
}

void TestMarkdownHtml::listMarkersStandOnTheBaseline()
{
    // A cell's valign leaks into an image's vertical alignment: AlignTop would pin the
    // marker to the line top, a few pixels above the text, more at larger scales.
    const auto doc = document("- plain\n- `chip` item\n  - nested");
    int markers = 0;
    for (auto block = doc->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            if (!format.isImageFormat()) continue;
            ++markers;
            QVERIFY2(format.verticalAlignment() == QTextCharFormat::AlignNormal || format.verticalAlignment() == QTextCharFormat::AlignBaseline,
                     qPrintable(QString::number(format.verticalAlignment())));
        }
    QCOMPARE(markers, 3);
}

void TestMarkdownHtml::tableStripesUseTheirOwnToken()
{
    auto theme = light(); theme.stripe = QColor("#123456");
    const auto output = MarkdownHtml::render("| A |\n|---|\n| 1 |\n| 2 |", theme, {});
    QCOMPARE(output.count("<tr bgcolor=\"#123456\">"), 1);
    QCOMPARE(light().stripe, light().subtle);   // GitHub's own zebra colour by default
}

QTEST_MAIN(TestMarkdownHtml)
#include "test_markdownhtml.moc"
