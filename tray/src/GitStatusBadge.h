#pragma once

#include "CachedText.h"
#include "WorkspaceIcons.h"
#include <QDateTime>
#include <QJsonObject>
#include <QFontMetrics>
#include <QList>

namespace GitStatusBadge {
enum Tone { Muted, Attention, Incoming, Synced, Conflict };
struct Mark { QString icon, count; Tone tone; };

inline qint64 ageMs(const QJsonObject &data) {
    return QDateTime::currentMSecsSinceEpoch() - qint64(data.value("received_at_ms").toDouble());
}
inline bool fresh(const QJsonObject &data) {
    return !data.value("offline").toBool() && ageMs(data) >= 0 && ageMs(data) < 30000 && data.value("state") == "ok"
        && data.value("changed_files").isDouble() && data.value("changed_files").toDouble() >= 0;
}
inline bool verified(const QJsonObject &data) {
    return fresh(data) && data.value("remote_state") == "verified"
        && data.value("remote_age_seconds").isDouble()
        && data.value("remote_age_seconds").toDouble() >= 0
        && data.value("remote_age_seconds").toDouble() + ageMs(data) / 1000. < 60;
}
inline QString countText(int count) { return count > 99 ? QStringLiteral("99+") : QString::number(count); }
inline QList<Mark> marks(const QJsonObject &data) {
    if (data.isEmpty() || data.value("state") == "not_repo" || data.value("archived").toBool()) return {};
    QList<Mark> result;
    const bool current = fresh(data), remote = verified(data);
    const int changed = data.value("changed_files").toInt(), ahead = data.value("ahead").toInt(), behind = data.value("behind").toInt();
    if (changed) result.append({"git-diff", countText(changed), current ? (data.value("conflicts").toInt() ? Conflict : Attention) : Muted});
    if (ahead) result.append({"git-push", countText(ahead), remote ? Attention : Muted});
    if (behind) result.append({"git-pull", countText(behind), remote ? Incoming : Muted});
    if (!remote) result.append({"git-unknown", {}, Muted});
    else if (!changed && !ahead && !behind && data.value("ahead").isDouble() && data.value("behind").isDouble()
        && !data.value("detached").toBool() && !data.value("unborn").toBool()) result.append({"git-synced", {}, Synced});
    return result;
}
inline QString tooltip(const QJsonObject &data) {
    if (data.isEmpty()) return {};
    QStringList lines{QObject::tr("Git status (current checkout)")};
    if (!data.value("root").toString().isEmpty()) lines << data.value("root").toString();
    if (!data.value("branch").toString().isEmpty()) lines << QObject::tr("Branch: %1").arg(data.value("branch").toString());
    if (!data.value("upstream").toString().isEmpty()) lines << QObject::tr("Upstream: %1").arg(data.value("upstream").toString());
    const auto state = data.value("state").toString(), remote = data.value("remote_state").toString();
    if (data.value("offline").toBool()) lines << QObject::tr("Computer offline. Last known values.");
    else if (!fresh(data) && data.value("changed_files").isDouble()) lines << QObject::tr("Last known values; waiting for a fresh check.");
    if (data.value("changed_files").isDouble()) lines << QObject::tr("%1 changed or new files (including staged and untracked files)").arg(data.value("changed_files").toInt());
    if (data.value("conflicts").toInt()) lines << QObject::tr("%1 files with merge conflicts").arg(data.value("conflicts").toInt());
    const QString reference = verified(data) ? QObject::tr("verified upstream") : QObject::tr("last known upstream; remote unverified");
    if (data.value("ahead").isDouble()) lines << QObject::tr("%1 outgoing commits (%2)").arg(data.value("ahead").toInt()).arg(reference);
    if (data.value("behind").isDouble()) lines << QObject::tr("%1 incoming commits (%2)").arg(data.value("behind").toInt()).arg(reference);
    if (state == "changed_during_check") lines << QObject::tr("Checkout changed during verification. Waiting for a new check.");
    else if (state != "ok") lines << QObject::tr("Git information unavailable.");
    else if (remote == "no_upstream") lines << QObject::tr("No upstream configured. Publication is unverified.");
    else if (remote == "local_upstream") lines << QObject::tr("Upstream is local. Remote publication is unverified.");
    else if (remote == "detached") lines << QObject::tr("Detached HEAD. No branch publication to verify.");
    else if (remote == "changed") lines << QObject::tr("Remote branch changed. Fetch to update local tracking information.");
    else if (remote == "missing") lines << QObject::tr("Remote branch does not exist.");
    else if (!verified(data)) lines << QObject::tr("Remote publication has not been verified recently.");
    else if (!data.value("changed_files").toInt() && !data.value("ahead").toInt() && !data.value("behind").toInt()) lines << QObject::tr("Clean checkout. Branch matches the verified remote.");
    if (data.value("remote_checked_at").toDouble() > 0) lines << QObject::tr("Remote checked: %1").arg(QDateTime::fromSecsSinceEpoch(qint64(data.value("remote_checked_at").toDouble())).toLocalTime().toString("d MMM HH:mm:ss"));
    return lines.join('\n');
}
inline QFont font(QFont base) { base.setPixelSize(11); base.setWeight(QFont::Medium); return base; }
inline int markWidth(const Mark &mark, QFont base) { return mark.count.isEmpty() ? 24 : 27 + CachedText::width(font(base), mark.count); }
inline int width(const QList<Mark> &values, QFont base) {
    int result = 0; for (const auto &mark : values) result += markWidth(mark, base) + 4;
    return qMax(0, result - 4);
}
inline QList<Mark> fitted(QList<Mark> values, QFont base, int available) {
    if (width(values, base) <= available) return values;
    // Overflow is distinct from unverified publication.
    if (values.size() > 1) values = {values.first(), {"more", {}, Muted}};
    if (width(values, base) > available) values = {{"more", {}, Muted}};
    return available >= 24 ? values : QList<Mark>();
}
inline QColor color(Tone tone, bool dark) {
    switch (tone) {
        case Attention: return QColor(dark ? "#f0c77b" : "#885400");
        case Incoming: return QColor(dark ? "#92bdf1" : "#356697");
        case Synced: return QColor(dark ? "#8bdfc0" : "#187454");
        case Conflict: return QColor(dark ? "#f1a0a2" : "#a7313a");
        default: return QColor(dark ? "#8794a3" : "#647386");
    }
}
inline void paint(QPainter *p, const QRect &rect, const QList<Mark> &values, QFont base, bool dark) {
    p->save(); p->setFont(font(base)); int x = rect.x();
    for (const auto &mark : values) {
        const int w = markWidth(mark, base); const QColor ink = color(mark.tone, dark);
        QColor fill = ink; fill.setAlpha(dark ? 24 : 18);
        const QRect badge(x, rect.y(), w, 18);
        p->setPen(Qt::NoPen); p->setBrush(fill); p->drawRoundedRect(badge, 4, 4);
        cachedWorkspaceIcon(mark.icon, ink).paint(p, QRect(x + 4, rect.y() + 1, 16, 16));
        if (!mark.count.isEmpty()) { p->setPen(ink); CachedText::draw(p, QRect(x + 23, rect.y(), w - 27, 18), Qt::AlignVCenter, mark.count); }
        x += w + 4;
    }
    p->restore();
}
}
