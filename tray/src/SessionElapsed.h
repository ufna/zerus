#pragma once

#include <QDateTime>
#include <QObject>
#include <QString>
#include <cmath>

namespace SessionElapsed {
inline QString text(double started, qint64 now = QDateTime::currentSecsSinceEpoch())
{
    if (!std::isfinite(started) || started <= 0) return {};
    const auto seconds = started >= now ? qint64(0) : now - qint64(started);
    if (seconds < 60) return QString("%1s").arg(seconds);
    if (seconds < 3600) return QString("%1m %2s").arg(seconds / 60).arg(seconds % 60, 2, 10, QLatin1Char('0'));
    if (seconds < 86400) return QString("%1h %2m").arg(seconds / 3600).arg(seconds / 60 % 60, 2, 10, QLatin1Char('0'));
    return QString("%1d %2h").arg(seconds / 86400).arg(seconds / 3600 % 24);
}
inline QString status(const QString &label, double started, qint64 now = QDateTime::currentSecsSinceEpoch())
{
    const auto elapsed = text(started, now);
    return label + (elapsed.isEmpty() ? QString() : " (" + elapsed + ")");
}
inline QString working(double started, qint64 now = QDateTime::currentSecsSinceEpoch()) { return status(QObject::tr("Working"), started, now); }
}
