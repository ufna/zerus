#include <QtTest>

#include <QFile>
#include <QTemporaryDir>
#include <QThread>

#include <atomic>
#include <csignal>
#include <memory>

#include "ProcessRunner.h"

// Helper processes start on ProcessRunner's worker: fork/exec of the large GUI
// process and child reaping must never run on the GUI thread, while each
// request still reports exactly once to its owner on the owner's thread.

class TestProcessRunner : public QObject {
    Q_OBJECT
private slots:
    void cleanup() { ProcessRunner::setLaunchObserver({}); }
    void launchesOnWorkerAndReportsOnOwnerThread();
    void capturesOutputErrorAndExitCode();
    void writesInputThenClosesStdin();
    void timeoutKillsChildAndReportsOnce();
    void missingProgramFailsToStart();
    void crashReportsProcessError();
    void destroyedOwnerGetsNoResultAndKillsChild();
};

namespace {
ProcessRunner::Request shell(const QString &script, int timeoutMs = 10000)
{
    ProcessRunner::Request request; request.program = QStringLiteral("/bin/sh");
    request.arguments = {QStringLiteral("-c"), script}; request.timeoutMs = timeoutMs; return request;
}
bool alive(qint64 pid) { return pid > 0 && ::kill(pid_t(pid), 0) == 0; }
qint64 readPid(const QString &path)
{
    QFile file(path); if (!file.open(QIODevice::ReadOnly)) return 0;
    return file.readAll().trimmed().toLongLong();
}
}

void TestProcessRunner::launchesOnWorkerAndReportsOnOwnerThread()
{
    std::atomic<QThread *> launchThread{nullptr};
    ProcessRunner::setLaunchObserver([&launchThread] { launchThread = QThread::currentThread(); });
    QObject owner; QThread *resultThread = nullptr; int results = 0; ProcessRunner::Result result;
    ProcessRunner::run(shell(QStringLiteral("printf hello")), &owner, [&](const ProcessRunner::Result &value) {
        ++results; resultThread = QThread::currentThread(); result = value;
    });
    QTRY_COMPARE(results, 1);
    QVERIFY(launchThread.load());
    QVERIFY(launchThread.load() != QThread::currentThread());
    QCOMPARE(launchThread.load(), ProcessRunner::workerThread());
    QCOMPARE(resultThread, QThread::currentThread());
    QCOMPARE(result.outcome, ProcessRunner::Result::Finished);
    QCOMPARE(result.exitStatus, QProcess::NormalExit);
    QCOMPARE(result.exitCode, 0);
    QCOMPARE(result.standardOutput, QByteArray("hello"));
}

void TestProcessRunner::capturesOutputErrorAndExitCode()
{
    QObject owner; int results = 0; ProcessRunner::Result result;
    // Larger than a pipe buffer: output is drained while the child runs.
    ProcessRunner::run(shell(QStringLiteral("head -c 200000 /dev/zero | tr '\\0' x; printf oops >&2; exit 3")), &owner,
        [&](const ProcessRunner::Result &value) { ++results; result = value; });
    QTRY_COMPARE(results, 1);
    QCOMPARE(result.outcome, ProcessRunner::Result::Finished);
    QCOMPARE(result.exitCode, 3);
    QCOMPARE(result.standardOutput, QByteArray(200000, 'x'));
    QCOMPARE(result.standardError, QByteArray("oops"));
    QCOMPARE(result.error, QProcess::UnknownError);
}

void TestProcessRunner::writesInputThenClosesStdin()
{
    QObject owner; int results = 0; ProcessRunner::Result withInput, empty;
    auto request = shell(QStringLiteral("cat")); request.input = QByteArray("{\"key\":\"value\"}");
    ProcessRunner::run(request, &owner, [&](const ProcessRunner::Result &value) { ++results; withInput = value; });
    // An empty payload still closes stdin, so readers see EOF instead of waiting.
    request.input = QByteArray(""); request.timeoutMs = 5000;
    ProcessRunner::run(request, &owner, [&](const ProcessRunner::Result &value) { ++results; empty = value; });
    QTRY_COMPARE(results, 2);
    QCOMPARE(withInput.standardOutput, QByteArray("{\"key\":\"value\"}"));
    QCOMPARE(empty.outcome, ProcessRunner::Result::Finished);
    QVERIFY(empty.standardOutput.isEmpty());
}

void TestProcessRunner::timeoutKillsChildAndReportsOnce()
{
    QTemporaryDir dir; QVERIFY(dir.isValid()); const auto pidFile = dir.filePath("pid");
    QObject owner; int results = 0; ProcessRunner::Result result;
    ProcessRunner::run(shell(QStringLiteral("echo $$ > '%1'; exec sleep 30").arg(pidFile), 300), &owner,
        [&](const ProcessRunner::Result &value) { ++results; result = value; });
    QTRY_VERIFY(readPid(pidFile) > 0);
    const auto pid = readPid(pidFile);
    QTRY_COMPARE_WITH_TIMEOUT(results, 1, 3000);
    QCOMPARE(result.outcome, ProcessRunner::Result::TimedOut);
    QTRY_VERIFY(!alive(pid));
    // The killed child's later exit is reaped without a second report.
    QTest::qWait(200);
    QCOMPARE(results, 1);
}

void TestProcessRunner::missingProgramFailsToStart()
{
    QObject owner; int results = 0; ProcessRunner::Result result;
    ProcessRunner::Request request; request.program = QStringLiteral("/no/such/program-for-zerus"); request.timeoutMs = 5000;
    ProcessRunner::run(request, &owner, [&](const ProcessRunner::Result &value) { ++results; result = value; });
    QTRY_COMPARE(results, 1);
    QCOMPARE(result.outcome, ProcessRunner::Result::FailedToStart);
    QCOMPARE(result.error, QProcess::FailedToStart);
    QVERIFY(!result.errorString.isEmpty());
    QTest::qWait(100);
    QCOMPARE(results, 1);
}

void TestProcessRunner::crashReportsProcessError()
{
    QObject owner; int results = 0; ProcessRunner::Result result;
    ProcessRunner::run(shell(QStringLiteral("kill -9 $$")), &owner, [&](const ProcessRunner::Result &value) { ++results; result = value; });
    QTRY_COMPARE(results, 1);
    QCOMPARE(result.outcome, ProcessRunner::Result::Finished);
    QCOMPARE(result.exitStatus, QProcess::CrashExit);
    QCOMPARE(result.error, QProcess::Crashed);
    QVERIFY(!result.errorString.isEmpty());
}

void TestProcessRunner::destroyedOwnerGetsNoResultAndKillsChild()
{
    QTemporaryDir dir; QVERIFY(dir.isValid()); const auto pidFile = dir.filePath("pid");
    auto owner = std::make_unique<QObject>(); int results = 0;
    ProcessRunner::run(shell(QStringLiteral("echo $$ > '%1'; exec sleep 30").arg(pidFile)), owner.get(),
        [&](const ProcessRunner::Result &) { ++results; });
    QTRY_VERIFY(readPid(pidFile) > 0);
    const auto pid = readPid(pidFile);
    owner.reset();
    // Destroying the owner kills its child before returning, like the old
    // HgsClient destructor, so temporary fixtures are not used afterwards.
    QVERIFY(!alive(pid));
    QTest::qWait(200);
    QCOMPARE(results, 0);
}

QTEST_GUILESS_MAIN(TestProcessRunner)
#include "test_processrunner.moc"
