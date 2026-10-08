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
