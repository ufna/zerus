#pragma once

#include "HgsClient.h"
#include <QObject>
#include <QDateTime>

namespace SessionPresentation {
inline QString status(const SessionInfo &s, bool reachable = true)
{
    if (!reachable) return QObject::tr("Offline");
    if (s.state == "archived") return QObject::tr("Archived");
    if (s.state == "paused") return QObject::tr("Paused");
    if (s.state == "stopped") return QObject::tr("Stopped");
    if (s.processState == "exited") return QObject::tr("Agent exited");
    if (!s.tracked && s.activity.isEmpty()) return QObject::tr("Not tracked");
    if (s.phase == "approval") return QObject::tr("Needs approval");
    if (s.phase == "input" && s.cmd == "dsh" && s.activitySummary == "Sign in required") return QObject::tr("Sign in");
    if (s.phase == "input") return QObject::tr("Needs input");
    if (s.recovery.value("state") == "waiting") {
        const auto seconds = qMax(0, int(s.recovery.value("due_at").toDouble()-QDateTime::currentMSecsSinceEpoch()/1000.0));
        return seconds ? QObject::tr("Retry in %1 s").arg(seconds) : QObject::tr("Waiting to retry");
    }
    if (s.recovery.value("state") == "dispatching") return QObject::tr("Retrying");
    if (s.phase == "error") return QObject::tr("Error");
    if (s.phase == "interrupted") return QObject::tr("Interrupted");
    if (s.phase == "compacting") return QObject::tr("Compacting");
    if (s.phase == "starting") return QObject::tr("Starting");
    if (s.phase == "tool") return QObject::tr("Using a tool");
    if (s.activity == "busy") return QObject::tr("Working");
    if (s.activity == "idle") return QObject::tr("Ready");
    // A conversation can end while the CLI and its tmux terminal remain alive.
    if (s.activity == "ended" || s.activity == "unknown") return QObject::tr("Status unknown");
    return QObject::tr("Connecting");
}

inline QString displayTitle(const SessionInfo &s)
{
    const QString project = s.project.isEmpty() ? s.name.section('/', 1, 1) : s.project;
    const QString tag = s.tag.isEmpty() ? s.name.section('/', 2) : s.tag;
    if (project.isEmpty()) return s.name;
    return tag.isEmpty() ? project : project + QStringLiteral(" / ") + tag;
}

inline QString sessionLabel(const SessionInfo &s)
{
    const QString tag = s.tag.isEmpty() ? s.name.section('/', 2) : s.tag;
    return tag.isEmpty() ? (s.project.isEmpty() ? s.name : s.project) : tag;
}

inline QString projectContext(const SessionInfo &s)
{
    QStringList parts{s.project.isEmpty() ? s.name.section('/', 1, 1) : s.project};
    if (!s.gitBranch.isEmpty()) parts << s.gitBranch;
    else if (s.gitDetached) parts << QObject::tr("detached HEAD");
    if (s.gitWorktree) parts << (s.gitWorktreeName.isEmpty() ? QObject::tr("worktree") : QObject::tr("worktree: %1").arg(s.gitWorktreeName));
    return parts.join(QStringLiteral(" / "));
}

inline bool currentActivity(const SessionInfo &s, bool online = true)
{
    return online && s.state == "running" && s.processState != "exited"
        && s.activity != "unknown" && !(s.conversationState == "ended" && s.activity != "idle");
}

inline QString currentAction(const SessionInfo &s, bool online = true)
{
    // A tool excerpt must not hide the fact that the agent is waiting for us.
    if (online && s.needsAction()) {
        const QString detail = !s.activityDetail.isEmpty() ? s.activityDetail : !s.toolDetail.isEmpty() ? s.toolDetail : s.prompt;
        return status(s) + (detail.isEmpty() ? QString() : QStringLiteral(": ") + detail);
    }
    if (s.unreadReply && s.activity != "busy" && !s.needsAction()) return status(s, online);
    if (!currentActivity(s, online)) return status(s, online);

    if (!s.activitySummary.isEmpty()) return s.activitySummary + (s.activityDetail.isEmpty() ? QString() : QStringLiteral(": ") + s.activityDetail);
    if (!s.currentTool.isEmpty()) return s.currentTool + (s.toolDetail.isEmpty() ? QString() : QStringLiteral(": ") + s.toolDetail);
    if (!s.prompt.isEmpty()) return status(s) + QStringLiteral(": ") + s.prompt;
    return s.tracked ? status(s) : QObject::tr("Open terminal to view activity");
}

inline QString childCount(const SessionInfo &s, bool expanded = false, bool online = true)
{
    if (!currentActivity(s, online) || s.subagentSource == "unavailable") return {};
    const int active = qMax(s.subagentActiveCount, s.subagentCount);
    if (s.subagentSource == "hook_profiles" && !s.subagentCountsComplete)
        return expanded ? QObject::tr("Observed runs: incomplete counts") : QStringLiteral("?");
    if (s.subagentSource == "hooks" && !s.subagentCountsComplete && s.subagentTotalCount > 0)
        return expanded ? QObject::tr("%1 confirmed active / %2 observed; some states unknown").arg(active).arg(s.subagentTotalCount)
                        : QString("%1+/%2").arg(active).arg(s.subagentTotalCount);
    if (s.subagentTotalCount > 0)
        return expanded ? (s.subagentSource == "hook_profiles" ? QObject::tr("%1 active / %2 observed runs") : QObject::tr("%1 active / %2 total")).arg(active).arg(s.subagentTotalCount) : QString("%1/%2").arg(active).arg(s.subagentTotalCount);
    if (s.subagentTotalCount < 0 && s.subagentSource == "hook_profiles")
        return expanded ? QObject::tr("Observed runs: count unavailable") : QStringLiteral("?");
    if (s.subagentTotalCount < 0 && active > 0)
        return expanded ? QObject::tr("%1 active / total unknown").arg(active) : QString("%1/?").arg(active);
    return {};
}

}
