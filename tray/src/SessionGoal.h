#pragma once

#include <QJsonObject>
#include <QLocale>
#include <QObject>
#include <QStringList>

namespace SessionGoal {
inline QString status(const QJsonObject &goal)
{
    const auto state = goal.value("status").toString();
    if (state == "active") return goal.value("activation") == "disarmed" ? QObject::tr("Goal waiting to resume") : QObject::tr("Pursuing goal");
    if (state == "paused") return QObject::tr("Goal paused");
    if (state == "blocked") return QObject::tr("Goal blocked");
    if (state == "usage_limited") return QObject::tr("Goal usage limit reached");
    if (state == "budget_limited") return QObject::tr("Goal token budget reached");
    if (state == "complete") return QObject::tr("Goal complete");
    return QObject::tr("Goal status unknown");
}
inline QString elapsed(const QJsonObject &goal)
{
    if (!goal.value("time_used_seconds").isDouble()) return {};
    const auto seconds = qMax<qint64>(0, goal.value("time_used_seconds").toInteger());
    if (seconds >= 86400) return QObject::tr("%1d %2h").arg(seconds / 86400).arg(seconds / 3600 % 24);
    if (seconds >= 3600) return QObject::tr("%1h %2m").arg(seconds / 3600).arg(seconds / 60 % 60);
    if (seconds >= 60) return QObject::tr("%1m").arg(seconds / 60);
    return QObject::tr("%1s").arg(seconds);
}
inline QString tokens(const QJsonObject &goal)
{
    if (!goal.value("tokens_used").isDouble()) return {};
    const auto used = QLocale().toString(goal.value("tokens_used").toInteger());
    const auto budget = goal.value("token_budget").toInteger();
    return budget > 0 ? QObject::tr("%1 / %2 tokens").arg(used, QLocale().toString(budget))
                      : QObject::tr("%1 tokens used").arg(used);
}
inline QStringList metrics(const QJsonObject &goal)
{
    QStringList result;
    if (!elapsed(goal).isEmpty()) result << QObject::tr("Recorded active time: %1").arg(elapsed(goal));
    if (!tokens(goal).isEmpty()) result << tokens(goal);
    for (const auto &pair : {qMakePair("rounds_used", "round_budget"), qMakePair("turns_used", "turn_budget")}) {
        if (!goal.value(pair.first).isDouble()) continue;
        QString used = QLocale().toString(goal.value(pair.first).toInteger());
        if (goal.value(pair.second).isDouble()) used += " / " + QLocale().toString(goal.value(pair.second).toInteger());
        result << (QString(pair.first) == "rounds_used" ? QObject::tr("Goal rounds: %1").arg(used) : QObject::tr("Continuation turns: %1").arg(used));
    }
    if (goal.value("evaluations").isDouble()) result << QObject::tr("Completion checks: %1").arg(goal.value("evaluations").toInteger());
    if (goal.value("time_budget_seconds").isDouble()) {
        QJsonObject time{{"time_used_seconds", goal.value("time_budget_seconds")}};
        result << QObject::tr("Time budget: %1").arg(elapsed(time));
    }
    return result;
}
}
