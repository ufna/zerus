#pragma once

#include "SessionList.h"
#include "IdentityBadge.h"
#include "ProjectAppearance.h"
#include "MachineAppearance.h"
#include "SessionStatusBadge.h"
#include "SessionElapsed.h"
#include "WorkspaceIcons.h"
#include <QPainter>
#include <QStyledItemDelegate>

// One renderer for session-list rows and overview cards.
inline QRect sessionCardRect(const QRect &row) { return row.adjusted(2, 4, -2, -6); }

class SessionDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;
    QSize sizeHint(const QStyleOptionViewItem &option, const QModelIndex &index) const override {
        return {280, !index.data(SessionRoles::ChildId).toString().isEmpty() ? 64 : index.data(SessionRoles::Header).toBool() ? 40 : option.widget && option.widget->property("compact").toBool() ? 94 : 104};
    }
    void paint(QPainter *p, const QStyleOptionViewItem &option, const QModelIndex &index) const override {
        p->save(); p->setRenderHint(QPainter::Antialiasing);
        const bool dark = option.widget && option.widget->property("hgsDark").toBool();
        if (index.data(SessionRoles::Header).toBool()) {
            const int attention = index.data(SessionRoles::Attention).toInt(), working = index.data(SessionRoles::Working).toInt(), unread = index.data(SessionRoles::Unread).toInt();
            const QRect band = option.rect.adjusted(2, 5, -2, -3), r = band.adjusted(9, 0, -9, 0);
            const QColor projectColor = index.data(SessionRoles::ProjectColor).value<QColor>();
            const bool vivid = index.data(SessionRoles::ProjectVivid).toBool();
            const auto fg = ProjectAppearance::ink(projectColor,dark,vivid);
            const auto background = ProjectAppearance::fill(projectColor,dark,vivid);
            p->setPen(Qt::NoPen); p->setBrush(background); p->drawRoundedRect(band,6,6);
            p->setPen(QPen(fg, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
            const QPoint c(r.x() + 3, r.center().y()); QPolygon arrow;
            if (index.data(SessionRoles::Collapsed).toBool()) arrow << c + QPoint(-2,-4) << c + QPoint(2,0) << c + QPoint(-2,4);
            else arrow << c + QPoint(-4,-2) << c + QPoint(0,2) << c + QPoint(4,-2);
            p->drawPolyline(arrow);
            workspaceIcon("projects", fg).paint(p, QRect(r.x() + 15, r.center().y() - 8, 16, 16));
            QFont font = option.font; font.setPixelSize(11); font.setWeight(QFont::Medium); p->setFont(font);
            int right = r.right();
            const auto counter = [&](const QString &text, const QColor &color, const QColor &background) {
                const int w = QFontMetrics(font).horizontalAdvance(text) + 12;
                const QRect badge(right - w, r.center().y() - 9, w, 18); p->setPen(Qt::NoPen); p->setBrush(background);
                p->drawRoundedRect(badge, 5, 5); p->setPen(color); p->drawText(badge, Qt::AlignCenter, text); right -= w + 6;
            };
            counter(QString::number(index.data(SessionRoles::Total).toInt()), fg, MachineAppearance::blend(background,fg,.13));
            if (attention) counter(QString("! %1").arg(attention), QColor("#392900"), QColor(dark ? "#ffda76" : "#f4ce65"));
            if (unread) counter(QString("● %1").arg(unread), QColor("#392900"), QColor(dark ? "#ffda76" : "#f4ce65"));
            if (working) {
                QColor progress(dark ? "#55e39a" : "#12834e");
                progress.setAlphaF(.72 + .28 * (option.widget ? option.widget->property("workingPulse").toReal() : 0.0));
                counter(QString("● %1").arg(working), progress, QColor(dark ? "#123624" : "#e0f5e9"));
            }
            font.setPixelSize(12); font.setWeight(QFont::DemiBold); p->setFont(font); p->setPen(fg);
            p->drawText(QRect(r.x() + 39, r.y(), qMax(0, right - r.x() - 39), r.height()), Qt::AlignVCenter,
                QFontMetrics(font).elidedText(index.data(SessionRoles::Title).toString(), Qt::ElideRight, qMax(0, right - r.x() - 39)));
            p->restore(); return;
        }
        const bool childRow = !index.data(SessionRoles::ChildId).toString().isEmpty();
        const bool expandable = index.data(SessionRoles::HasChildren).toBool();
        const QRect r = sessionCardRect(option.rect).adjusted(childRow ? 24 : 0, 0, 0, 0);
        const bool compact = option.widget && option.widget->property("compact").toBool();
        const bool attention = index.data(SessionRoles::Attention).toBool();
        const bool anyUnread = index.data(SessionRoles::Unread).toBool();
        const bool unread = index.data(SessionRoles::Unread).toBool();
        const bool error = index.data(SessionRoles::Status).toString() == "Error";
        const bool working = index.data(SessionRoles::Working).toInt() > 0;
        const QString state = index.data(SessionRoles::Status).toString();
        const bool paused = state == "Paused";
        p->setClipRect(option.rect);
        QStyleOptionViewItem cardOption(option); cardOption.rect.adjust(childRow ? 24 : 0, 2, 0, -4);
        IdentityBadges::paintRow(p, cardOption, dark, attention ? (error ? IdentityBadges::Error : IdentityBadges::Attention) : anyUnread ? IdentityBadges::Unread : IdentityBadges::Normal);
        const int x = r.x() + 12, textX = x + 8, right = r.right() - 11, width = right - textX;
        const QColor foreground(dark ? "#e8edf4" : "#1a2733"), muted(dark ? "#a1adbb" : "#647386");
        if (childRow) {
            p->setPen(QPen(QColor(dark ? "#38434e" : "#c5cfd9"), 1));
            p->drawLine(option.rect.left() + 17, option.rect.top(), option.rect.left() + 17, option.rect.bottom());
            p->drawLine(option.rect.left() + 17, r.center().y(), r.left() - 3, r.center().y());
        }
        const int titleY = r.y() + (childRow ? (compact ? 5 : 6) : (compact ? 3 : 5));
        const int metadataY = titleY + (compact ? 22 : 24);
        const int detailY = metadataY + (compact ? 20 : 22);
        const int modelY = option.rect.bottom() - 26;

        // Provider goes with the session title; machine goes with its folder.
        // At narrow widths the provider keeps its icon, leaving room for the
        // title and elapsed status. The footer is independent of those badges.
        const auto kind = attention ? (error ? SessionStatusBadge::Error : SessionStatusBadge::Attention)
            : working ? SessionStatusBadge::Working : unread ? SessionStatusBadge::Unread
            : paused ? SessionStatusBadge::Paused : SessionStatusBadge::Neutral;
        const QString caption = working ? SessionElapsed::status(state == "Compacting" ? tr("Compacting") : tr("Working"), index.data(SessionRoles::WorkingSince).toDouble()) : unread && !attention ? tr("New reply")
            : state == "Not tracked" ? tr("Untracked") : state;
        const bool additionalUnread = unread && (working || attention);
        const int statusWidth = SessionStatusBadge::width(caption, kind, option.font);
        QFont font = option.font; font.setPixelSize(14); font.setWeight(QFont::DemiBold);
        const QString agent = index.data(SessionRoles::Agent).toString(), host = index.data(SessionRoles::Host).toString();
        const int reservedTitle = qMin(72, QFontMetrics(font).horizontalAdvance(index.data(SessionRoles::Title).toString()));
        const int providerRoom = right - x - statusWidth - 16 - (additionalUnread ? 28 : 0) - reservedTitle;
        const int providerNaturalWidth = agent.isEmpty() ? 0 : IdentityBadges::width(IdentityBadges::Provider, agent, option.font, childRow ? 85 : 95);
        const int providerWidth = providerNaturalWidth && !childRow && providerRoom < providerNaturalWidth ? 26 : providerNaturalWidth;
        const int titleWidth = qMax(0, right - x - statusWidth - 8 - (additionalUnread ? 28 : 0) - (providerWidth ? providerWidth + 8 : 0));
        p->setFont(font); p->setPen(foreground);
        const QString title = QFontMetrics(font).elidedText(index.data(SessionRoles::Title).toString(), Qt::ElideRight, titleWidth);
        p->drawText(QRect(x, titleY, titleWidth, 21), Qt::AlignVCenter, title);
        const QRect statusBadge(x + QFontMetrics(font).horizontalAdvance(title) + 8, titleY, statusWidth, 20);
        SessionStatusBadge::paint(p, statusBadge, caption, kind, dark,
            option.widget ? option.widget->property("workingPulse").toReal() : 0.0);
        if (additionalUnread) SessionStatusBadge::paint(p, QRect(statusBadge.right() + 6, titleY, 22, 20), {}, SessionStatusBadge::Unread, dark);
        if (providerWidth) IdentityBadges::paint(p, QRect(right - providerWidth, titleY + 1, providerWidth, 18), IdentityBadges::Provider, agent, dark);

        if (childRow) {
            font.setPixelSize(11); font.setWeight(QFont::Normal); p->setFont(font); p->setPen(muted);
            p->drawText(QRect(x, titleY + 25, right - x, 18), Qt::AlignVCenter,
                QFontMetrics(font).elidedText(index.data(SessionRoles::Detail).toString().simplified(), Qt::ElideRight, right - x));
            p->restore(); return;
        }
        font.setPixelSize(11); font.setWeight(QFont::Normal); p->setFont(font);
        const int hostWidth = IdentityBadges::width(IdentityBadges::Machine, host, font, width / 3 + 10);
        const QRect machineBadge(right - hostWidth, metadataY, hostWidth, 18);
        IdentityBadges::paint(p, machineBadge, IdentityBadges::Machine, host, dark, index.data(SessionRoles::MachineColor).value<QColor>(), index.data(SessionRoles::MachineName).toString());
        p->setPen(muted);
        const int metadataWidth = qMax(0, machineBadge.left() - textX - 8);
        p->drawText(QRect(textX, metadataY, metadataWidth, 18), Qt::AlignVCenter,
                    QFontMetrics(font).elidedText(index.data(SessionRoles::Meta).toString(), Qt::ElideMiddle, metadataWidth));

        QString detail = index.data(SessionRoles::Detail).toString();
        // Status is already beside the title. Keep the activity excerpt useful
        // without repeating "Working" or treating an old tool as a new reply.
        if (detail == state) detail.clear();
        else if (detail.startsWith(state + ": ")) detail.remove(0, state.size() + 2);
        if (detail.isEmpty() && unread && !working && !attention) detail = tr("Response ready to read");
        else if (detail.isEmpty() && state == "Ready") detail = tr("Waiting for your next message");
        else if (detail.isEmpty() && state == "Not tracked") detail = tr("Open terminal to view activity");
        else if (detail.isEmpty() && paused) detail = tr("Resume to continue");
        const bool reviewLater = index.data(SessionRoles::ReviewLater).toBool();
        const int detailX = textX + (reviewLater ? 18 : 0);
        if (reviewLater) workspaceIcon("attention", QColor(dark ? "#f0c77b" : "#885400")).paint(p, QRect(textX, detailY + 2, 14, 14));
        const int detailWidth = width - (detailX - textX);
        font.setPixelSize(12); p->setFont(font); p->setPen(state == "Offline" ? muted : foreground);
        p->drawText(QRect(detailX, detailY, detailWidth, 18), Qt::AlignVCenter,
                    QFontMetrics(font).elidedText(detail, Qt::ElideRight, detailWidth));

        font.setPixelSize(11); p->setFont(font); p->setPen(muted);
        const QString children = SessionList::childrenLabel(index);
        const QRect childrenControl = SessionList::childrenControlRect(option.rect, index, option.font);
        const int childWidth = expandable ? childrenControl.width() : children.isEmpty() ? 0 : QFontMetrics(font).horizontalAdvance(children) + 32;
        if (!children.isEmpty()) {
            const QRect counter = expandable ? childrenControl : QRect(right - childWidth, modelY, childWidth, 20);
            const bool hovered = expandable && option.widget && option.widget->property("hoveredChildren").toString() == index.data(SessionRoles::Key).toString();
            if (expandable) {
                p->setPen(Qt::NoPen); p->setBrush(QColor(dark ? (hovered ? "#344650" : "#28323c") : (hovered ? "#d4e4e9" : "#e7edf2")));
                p->drawRoundedRect(counter, 4, 4);
            }
            const QColor ink = hovered ? foreground : muted;
            const qreal cx = counter.x() + 10, cy = counter.y() + counter.height() / 2.0;
            p->setPen(QPen(ink, 1.2)); p->setBrush(Qt::NoBrush);
            p->drawEllipse(QPointF(cx, cy - 3), 2, 2); p->drawEllipse(QPointF(cx + 6, cy - 2), 2, 2);
            p->drawArc(QRectF(cx - 4, cy, 8, 6), 0, 180 * 16); p->drawArc(QRectF(cx + 2, cy + 1, 7, 5), 0, 180 * 16);
            p->drawText(counter.adjusted(26, 0, expandable ? -20 : -6, 0), Qt::AlignCenter, children);
            if (expandable) {
                p->setPen(QPen(ink, 1.4, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
                const QPointF c(counter.right() - 9, cy); QPolygonF arrow;
                const int direction = index.data(SessionRoles::Expanded).toBool() ? -1 : 1;
                arrow << c + QPoint(-3, -direction) << c + QPoint(0, 2 * direction) << c + QPoint(3, -direction);
                p->drawPolyline(arrow);
            }
        }
        font.setPixelSize(10); p->setFont(font);
        const QString effort = index.data(SessionRoles::Effort).toString();
        const bool hasEffort = !effort.isEmpty() && effort != "—";
        const int effortWidth = hasEffort ? QFontMetrics(font).horizontalAdvance(effort) + 10 : 0;
        const int modelWidth = qMax(0, width - (childWidth ? childWidth + 10 : 0) - (hasEffort ? effortWidth + 7 : 0));
        const QString model = QFontMetrics(font).elidedText(index.data(SessionRoles::Model).toString(), Qt::ElideMiddle, modelWidth);
        p->setPen(muted); p->drawText(QRect(textX, modelY, modelWidth, 20), Qt::AlignVCenter, model);
        if (hasEffort) {
            const QRect effortBadge(textX + QFontMetrics(font).horizontalAdvance(model) + 7, modelY + 2, effortWidth, 16);
            p->setPen(Qt::NoPen); p->setBrush(QColor(dark ? "#2b343e" : "#e7edf2")); p->drawRoundedRect(effortBadge, 3, 3);
            p->setPen(muted); p->drawText(effortBadge, Qt::AlignCenter, effort);
        }
        p->restore();
    }
};
