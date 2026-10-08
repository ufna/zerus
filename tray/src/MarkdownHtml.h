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
