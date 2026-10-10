#pragma once

#include <QByteArray>
#include <QProcess>
#include <QProcessEnvironment>
#include <QString>
#include <QStringList>

#include <functional>
#include <optional>

class QObject;
class QThread;

// Starts short-lived helper processes on one shared worker thread. fork/exec of
// the large GUI process, pipe I/O and child reaping then never stall painting:
// the GUI thread only posts a request and later receives one result. Polls start
// about one hgs child per second, and on macOS each fork() cost ~4 ms there.
namespace ProcessRunner {

struct Request {
    QString program;
    QStringList arguments;
    std::optional<QProcessEnvironment> environment;
    // Written once the child starts, then stdin is closed; an empty value only
    // closes it. Without a value stdin stays open, as with a plain QProcess.
    std::optional<QByteArray> input;
    int timeoutMs = 0;   // 0: no deadline
};

struct Result {
    enum Outcome { Finished, FailedToStart, TimedOut };
    Outcome outcome = Finished;
    int exitCode = -1;
    QProcess::ExitStatus exitStatus = QProcess::NormalExit;
    // The first QProcess error, e.g. Crashed before a CrashExit finish.
    QProcess::ProcessError error = QProcess::UnknownError;
    QString errorString;
    QByteArray standardOutput, standardError;
};

// Reports exactly once on owner's thread, which must be the application thread.
// A deadline kills the child and reports TimedOut at once. Destroying owner
// kills and reaps its running children before returning and drops their results.
void run(Request request, QObject *owner, std::function<void(const Result &)> done);

// Tests: the worker thread, and a hook run there right before each start.
QThread *workerThread();
void setLaunchObserver(std::function<void()> observer);

}
