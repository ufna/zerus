#include "MarkdownObjects.h"

#include <QAbstractTextDocumentLayout>
#include <QFontMetricsF>
#include <QGuiApplication>
#include <QHash>
#include <QPainter>
#include <QRegularExpression>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextDocumentFragment>
#include <QTextFragment>
#include <QTextObjectInterface>
#include <QTextTable>
#include <QUrlQuery>

#include <cmath>
#include <functional>

namespace {
enum : int { ChipText = QTextFormat::UserProperty + 41, ChipFill, ChipInk, ChipFamily, ChipScale, ChipPixels, ChipHeading,
             ChipWeight, ChipItalic };
constexpr int MaximumChipLength = 40;

QFont chipFont(const QTextFormat &format)
{
    QFont font(format.stringProperty(ChipFamily));
    font.setPixelSize(qMax(1, format.intProperty(ChipPixels)));
    if (format.intProperty(ChipWeight) > 0) font.setWeight(QFont::Weight(format.intProperty(ChipWeight)));
    font.setItalic(format.boolProperty(ChipItalic));
    return font;
}
// GitHub pads code .2em .4em, and 0 .2em inside headings.
QPointF chipPadding(const QTextFormat &format)
{
    const qreal em = format.intProperty(ChipPixels);
    return format.boolProperty(ChipHeading) ? QPointF(0.2 * em, 0) : QPointF(0.4 * em, 0.2 * em);
}

class ChipHandler final : public QObject, public QTextObjectInterface {
    Q_OBJECT
    Q_INTERFACES(QTextObjectInterface)
public:
    using QObject::QObject;
    QSizeF intrinsicSize(QTextDocument *, int, const QTextFormat &format) override
    {
        const QFontMetricsF metrics(chipFont(format)); const QPointF padding = chipPadding(format);
        return {metrics.horizontalAdvance(format.stringProperty(ChipText)) + 2 * padding.x(), metrics.ascent() + metrics.descent() + 2 * padding.y()};
    }
    void drawObject(QPainter *painter, const QRectF &rect, QTextDocument *, int, const QTextFormat &format) override
    {
        const QFont font = chipFont(format); const QFontMetricsF metrics(font);
        const QPointF padding = chipPadding(format); const qreal radius = 6 * format.doubleProperty(ChipScale);
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing);
        painter->setPen(Qt::NoPen); painter->setBrush(format.colorProperty(ChipFill));
        painter->drawRoundedRect(rect, radius, radius);
        painter->setFont(font); painter->setPen(format.colorProperty(ChipInk));
        painter->drawText(QPointF(rect.x() + padding.x(), rect.y() + padding.y() + metrics.ascent()), format.stringProperty(ChipText));
        painter->restore();
    }
};

void install(QTextDocument *document)
{
    auto *layout = document->documentLayout();
    if (layout->findChild<ChipHandler *>()) return;
    layout->registerHandler(MarkdownObjects::ChipObjectType, new ChipHandler(layout));
}

// Qt centres an AlignMiddle object xHeight / 4 above the baseline. GitHub pads
// the code symmetrically around text on the line's baseline, so the chip centre
// lies (ascent − descent) / 2 above it; pick the format font that puts it there.
QFont alignmentFont(const QFont &code)
{
    static QHash<int, QFont> cache;
    const QFontMetricsF metrics(code);
    const int key = qRound((metrics.ascent() - metrics.descent()) * 50);   // 4 × centre, in 1/100 px
    if (const auto found = cache.constFind(key); found != cache.cend()) return *found;
    QFont font = QGuiApplication::font();
    for (int pixels = 1; pixels < 400; ++pixels) {
        font.setPixelSize(pixels);
        if (QFontMetricsF(font).xHeight() * 100 >= key) break;
    }
    cache.insert(key, font);
    return font;
}

QString markerText(const QString &imageName)
{
    if (!imageName.startsWith(QLatin1String("hgs-md:"))) return QString();
    const QString name = QUrl(imageName).path().section('/', 0, 0);
    if (name == "disc") return QString::fromUtf8("• ");
    if (name == "circle") return QString::fromUtf8("◦ ");
    if (name == "square") return QString::fromUtf8("▪ ");
    if (name == "check-on") return QStringLiteral("[x] ");
    if (name == "check-off") return QStringLiteral("[ ] ");
    if (name == "gap") return QStringLiteral(" ");
    return QString();
}

QTextTable *tableOf(const QTextBlock &block)
{
    return block.isValid() ? QTextCursor(block).currentTable() : nullptr;
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
    auto theme = MarkdownTheme::github(dark, QColor(dark ? "#0d1117" : "#ffffff"), scale);
    if (url.hasQuery()) {
        static const QRegularExpression hex(QStringLiteral("^[0-9a-f]{6}$"));
        const QString fill = QUrlQuery(url).queryItemValue(QStringLiteral("fill"));
        if (!hex.match(fill).hasMatch()) return {};
        theme.subtle = QColor('#' + fill);
    }
    const qreal ratio = qMax<qreal>(1.0, devicePixelRatio);
    const auto draw = [&](qreal width, qreal height, const std::function<void(QPainter &)> &paint) {
        QImage image(qCeil(width * scale * ratio), qCeil(height * scale * ratio), QImage::Format_ARGB32_Premultiplied);
        image.setDevicePixelRatio(ratio); image.fill(Qt::transparent);
        QPainter painter(&image); painter.setRenderHint(QPainter::Antialiasing); painter.scale(scale, scale);
        paint(painter);
        return image;
    };
    const QString &name = parts[0];
    // Markers fill the 28 px list gutter; the dot sits 5.5 px above the image bottom (the baseline).
    if (name == "disc") return draw(28, 12, [&](QPainter &p) { p.setPen(Qt::NoPen); p.setBrush(theme.fg); p.drawEllipse(QRectF(11, 4, 5, 5)); });
    if (name == "circle") return draw(28, 12, [&](QPainter &p) { p.setPen(QPen(theme.fg, 1)); p.setBrush(Qt::NoBrush); p.drawEllipse(QRectF(11.5, 4.5, 4, 4)); });
    if (name == "square") return draw(28, 12, [&](QPainter &p) { p.fillRect(QRectF(11, 4, 5, 5), theme.fg); });
    // Centred on the line (vertical-align: middle), like GitHub's task checkbox.
    if (name == "check-on" || name == "check-off") return draw(28, 18, [&](QPainter &p) {
        const bool on = name == "check-on";
        p.setPen(QPen(theme.muted, 1)); p.setBrush(on ? theme.muted : theme.canvas);
        p.drawRoundedRect(QRectF(8.5, 1.5, 12, 12), 2, 2);
        if (on) {
            p.setPen(QPen(theme.canvas, 1.6, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
            p.drawPolyline(QPolygonF{QPointF(11.5, 7.5), QPointF(13.5, 9.5), QPointF(17.5, 5.0)});
        }
    });
    // Ordered markers are text right-aligned in the gutter between two transparent spacers.
    if (name == "blank" || name == "gap") return draw(1, 1, [](QPainter &) {});
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
            if (format.background().style() == Qt::NoBrush) continue;
            const QColor background = format.background().color();
            if (background != MarkdownHtml::chipSentinel() && background != MarkdownHtml::headingChipSentinel()) continue;
            if (!runs.isEmpty() && runs.last().position + runs.last().length == fragment.position()) {
                runs.last().length += fragment.length(); runs.last().text += fragment.text();
            } else runs.append({fragment.position(), fragment.length(), fragment.text(), format});
        }
    if (runs.isEmpty()) return 0;
    const double scale = theme.scalePercent / 100.0;
    if (objects) install(document);
    // One edit block: per-run edits would each relayout the document (seconds on long journals).
    QTextCursor cursor(document);
    cursor.beginEditBlock();
    for (auto it = runs.crbegin(); it != runs.crend(); ++it) {
        cursor.setPosition(it->position); cursor.setPosition(it->position + it->length, QTextCursor::KeepAnchor);
        QString text = it->text; text.replace(QChar::Nbsp, ' ');
        // The renderer sized and coloured the run after its surroundings (already content-scaled).
        const int pixels = it->format.font().pixelSize() > 0 ? it->format.font().pixelSize() : qRound(12 * scale);
        const QColor ink = it->format.hasProperty(QTextFormat::ForegroundBrush) ? it->format.foreground().color() : theme.fg;
        if (!objects || text.size() > MaximumChipLength) {
            // Plain spaces again, so that find() matches and long runs can wrap.
            QTextCharFormat format = it->format;
            format.setBackground(theme.chip); format.setForeground(ink);
            format.setFontFamilies(QStringList{theme.monoFamily});
            format.setProperty(QTextFormat::FontPixelSize, pixels);
            cursor.insertText(text, format);
            continue;
        }
        QTextCharFormat format;
        format.setObjectType(ChipObjectType);
        format.setProperty(ChipText, text); format.setProperty(ChipFill, theme.chip); format.setProperty(ChipInk, ink);
        format.setProperty(ChipFamily, theme.monoFamily); format.setProperty(ChipScale, scale); format.setProperty(ChipPixels, pixels);
        format.setProperty(ChipHeading, it->format.background().color() == MarkdownHtml::headingChipSentinel());
        format.setProperty(ChipWeight, it->format.fontWeight()); format.setProperty(ChipItalic, it->format.fontItalic());
        format.setVerticalAlignment(QTextCharFormat::AlignMiddle); format.setFont(alignmentFont(chipFont(format)));
        if (it->format.isAnchor()) {
            format.setAnchor(true); format.setAnchorHref(it->format.anchorHref()); format.setToolTip(it->format.toolTip());
        }
        cursor.insertText(QString(QChar::ObjectReplacementCharacter), format);
    }
    cursor.endEditBlock();
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
        result += line + '\n';
    }
    if (result.endsWith('\n')) result.chop(1);
    return result;
}

#include "MarkdownObjects.moc"
