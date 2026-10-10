#pragma once
#include <QCache>
#include <QFont>
#include <QFontMetrics>
#include <QPainter>
#include <QPaintDevice>
#include <QTextLayout>

// Cards repaint on every scroll and resize frame, and shaping the same short
// strings again took about a fifth of each frame. This keeps plain single-line
// text measured, elided and laid out. draw() follows the QTextLayout path of
// QPainter::drawText(rect, flags, text) and leaves anything that path treats
// differently to drawText itself. GUI thread only; caches are bounded LRUs.
namespace CachedText {
struct Key {
    QString text; QFont font; int detail;
    bool operator==(const Key &other) const { return detail == other.detail && text == other.text && font == other.font; }
};
inline size_t qHash(const Key &key, size_t seed = 0) { return qHashMulti(seed, key.text, key.font, key.detail); }
struct State {
    QCache<Key, qreal> advances{4096};
    QCache<Key, QString> elisions{2048};
    QCache<Key, QTextLayout> layouts{1024};
    quint64 misses = 0;
};
// Never destroyed: cached fonts and layouts must not outlive QGuiApplication at exit.
inline State &state() { static auto *value = new State; return *value; }
// Shaping work done on behalf of callers; tests use it to prove reuse.
inline quint64 misses() { return state().misses; }

// Point sizes depend on the device; only pixel-sized fonts share measurements.
inline qreal advance(const QFont &font, const QString &text) {
    if (font.pixelSize() <= 0) return QFontMetricsF(font).horizontalAdvance(text);
    auto &cache = state();
    const Key key{text, font, 0};
    if (const qreal *value = cache.advances.object(key)) return *value;
    ++cache.misses;
    const qreal value = QFontMetricsF(font).horizontalAdvance(text);
    cache.advances.insert(key, new qreal(value));
    return value;
}
// QFontMetrics rounds the same fixed-point advance.
inline int width(const QFont &font, const QString &text) { return qRound(advance(font, text)); }
inline QString elided(const QFont &font, const QString &text, Qt::TextElideMode mode, int width) {
    if (font.pixelSize() <= 0 || text.contains(QChar(0x9c))) return QFontMetrics(font).elidedText(text, mode, width);
    // elidedText returns text that fits unchanged, compared on the same advance.
    if (mode == Qt::ElideNone || text.size() <= 1 || advance(font, text) <= width) return text;
    auto &cache = state();
    const Key key{text, font, width * 4 + int(mode)};
    if (const QString *value = cache.elisions.object(key)) return *value;
    ++cache.misses;
    const QString value = QFontMetrics(font).elidedText(text, mode, width);
    cache.elisions.insert(key, new QString(value));
    return value;
}
inline void draw(QPainter *p, const QRectF &r, int flags, const QString &text) {
    if (!p->isActive() || text.isEmpty() || p->pen().style() == Qt::NoPen) return;
    const QFont &font = p->font();
    constexpr int supported = Qt::AlignLeft | Qt::AlignHCenter | Qt::AlignTop | Qt::AlignVCenter | Qt::AlignBottom;
    bool plain = !(flags & ~supported) && p->layoutDirection() == Qt::LeftToRight && !text.back().isSpace()
        && !font.underline() && !font.overline() && !font.strikeOut();
    for (qsizetype i = 0; plain && i < text.size(); ++i) {
        const char16_t c = text.at(i).unicode();
        plain = c != u'\n' && c != u'\r' && c != u'\t' && c != 0x9c && c != 0x2028 && c != 0x2029;
    }
    // Wrapping, mnemonics, tabs, several lines and right alignment keep Qt's path.
    if (!plain) { p->drawText(r, flags, text); return; }
    auto &cache = state();
    const Key key{text, font, font.pixelSize() > 0 ? 0 : p->device()->logicalDpiY()};
    QTextLayout *layout = cache.layouts.object(key);
    if (!layout) {
        ++cache.misses;
        layout = new QTextLayout(text, font);
        QTextOption option; option.setTextDirection(Qt::LeftToRight); layout->setTextOption(option);
        layout->setCacheEnabled(true);
        layout->beginLayout();
        QTextLine line = layout->createLine(); line.setLineWidth(0x01000000); line.setPosition(QPointF(0, 0));
        layout->endLayout();
        cache.layouts.insert(key, layout);
    }
    // The same offsets and clip as qt_format_text for one unwrapped line.
    const QTextLine line = layout->lineAt(0);
    const qreal height = line.ascent() + line.descent(), natural = line.naturalTextWidth();
    const qreal y = flags & Qt::AlignBottom ? r.height() - height : flags & Qt::AlignVCenter ? (r.height() - height) / 2 : 0;
    const bool centered = flags & Qt::AlignHCenter;
    const bool clip = !r.contains(QRectF(r.x() + (centered ? (r.width() - natural) / 2 : 0), r.y() + y, natural, height));
    if (clip) { p->save(); p->setClipRect(r, Qt::IntersectClip); }
    line.draw(p, QPointF(r.x() + (centered ? (r.width() - line.horizontalAdvance()) / 2 : 0), r.y() + y));
    if (clip) p->restore();
}
}
