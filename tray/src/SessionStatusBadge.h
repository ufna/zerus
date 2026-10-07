#pragma once
#include <QFontMetrics>
#include <QPainter>

namespace SessionStatusBadge {
enum Kind { Neutral, Working, Attention, Error, Paused, Unread };
inline QFont font(QFont base) { base.setPixelSize(11); base.setWeight(QFont::Medium); return base; }
inline int width(const QString &text, Kind kind, const QFont &base) {
    // Leave room for fractional glyph advances before elidedText rounds them.
    return QFontMetrics(font(base)).horizontalAdvance(text) + 16 + (kind == Neutral ? 0 : 16);
}
inline void paint(QPainter *p, const QRect &rect, const QString &text, Kind kind, bool dark, qreal pulse = 0) {
    p->save(); p->setRenderHint(QPainter::Antialiasing); p->setClipRect(rect, Qt::IntersectClip);
    const QColor background = kind == Working ? QColor(dark ? "#194d36" : "#ccefdc")
        : (kind == Attention || kind == Unread) ? QColor(dark ? "#ffda76" : "#f4ce65")
        : kind == Error ? QColor(dark ? "#ffabb6" : "#b73750")
        : kind == Paused ? QColor(dark ? "#443654" : "#e8ddf5") : QColor(dark ? "#2b343e" : "#e7edf2");
    const QColor foreground = kind == Working ? QColor(dark ? "#97f0ba" : "#145b37")
        : (kind == Attention || kind == Unread) ? QColor("#392900")
        : kind == Error ? QColor(dark ? "#46202a" : "#ffffff")
        : kind == Paused ? QColor(dark ? "#dec3ff" : "#634187") : QColor(dark ? "#b6c2d0" : "#566575");
    p->setPen(Qt::NoPen); p->setBrush(background); p->drawRoundedRect(rect, 5, 5);
    const QPointF center(rect.x() + 11, rect.center().y() + .5);
    p->setFont(font(p->font())); p->setPen(foreground);
    if (kind == Working) {
        p->setPen(Qt::NoPen); QColor halo(foreground); halo.setAlphaF(.08 + .25 * pulse);
        p->setBrush(halo); p->drawEllipse(center, 4 + pulse, 4 + pulse);
        p->setBrush(foreground); p->drawEllipse(center, 2.5, 2.5);
    } else if (kind == Attention || kind == Error) {
        auto bold = p->font(); bold.setBold(true); p->setFont(bold);
        p->drawText(QRectF(center.x() - 5, rect.y(), 10, rect.height()), Qt::AlignCenter, "!");
    } else if (kind == Paused) {
        p->setPen(QPen(foreground, 1.5, Qt::SolidLine, Qt::RoundCap));
        p->drawLine(center + QPointF(-2, -3), center + QPointF(-2, 3));
        p->drawLine(center + QPointF(2, -3), center + QPointF(2, 3));
    } else if (kind == Unread) {
        p->setPen(QPen(foreground, 1)); p->setBrush(Qt::NoBrush);
        p->drawRoundedRect(QRectF(center.x() - 4, center.y() - 3, 8, 6), 1, 1);
        p->drawLine(center + QPointF(-4, -3), center); p->drawLine(center, center + QPointF(4, -3));
    }
    p->setPen(foreground); p->setFont(font(p->font()));
    const QRect label = rect.adjusted(kind == Neutral ? 7 : 23, 0, -7, 0);
    p->drawText(label, Qt::AlignVCenter, QFontMetrics(p->font()).elidedText(text, Qt::ElideRight, qMax(0, label.width())));
    p->restore();
}
}
