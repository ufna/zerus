#include "ProcessRunner.h"

#include <QCoreApplication>
#include <QHash>
#include <QMutex>
#include <QObject>
#include <QPointer>
#include <QThread>
#include <QTimer>

#include <atomic>
#include <utility>

namespace ProcessRunner {
namespace {

// Everything below the worker object is touched only on the worker thread;
// pending results only on the application thread. The two sides talk through
// queued calls, so neither needs locks beyond the test observer.
struct Job {
    QProcess *process = nullptr;
    QTimer *timer = nullptr;
    bool reported = false;
    Result result;
};

struct Pending {
    QPointer<QObject> owner;
    std::function<void(const Result &)> done;
    QMetaObject::Connection destroyed;
};

class Runner {
public:
    Runner()
    {
        m_thread.setObjectName(QStringLiteral("hgs-process-runner"));
        m_worker = new QObject; m_worker->moveToThread(&m_thread);
        QObject::connect(&m_thread, &QThread::finished, m_worker, &QObject::deleteLater);
        m_thread.start();
    }
    void run(Request request, QObject *owner, std::function<void(const Result &)> done)
    {
        const quint64 id = ++m_next;
        Pending pending{owner, std::move(done), {}};
        // Owner destruction kills the child synchronously, as the old per-object
        // QProcess children did, so fixtures and sockets are not used afterwards.
        pending.destroyed = QObject::connect(owner, &QObject::destroyed, &m_front, [this, id] { cancel(id); },
                                             Qt::DirectConnection);
        m_pending.insert(id, std::move(pending));
        QMetaObject::invokeMethod(m_worker, [this, id, request = std::move(request)]() mutable { start(id, std::move(request)); },
                                  Qt::QueuedConnection);
    }
    void stop()
    {
        for (auto it = m_pending.begin(); it != m_pending.end(); ++it) QObject::disconnect(it->destroyed);
        m_pending.clear();
        // Quit must not leave children running or unreaped: kill and reap them on
        // the worker, then stop its loop. SIGKILL makes each wait short.
        QMetaObject::invokeMethod(m_worker, [this] {
            for (auto *job : std::as_const(m_jobs)) discard(job);
            qDeleteAll(m_jobs); m_jobs.clear();
        }, Qt::BlockingQueuedConnection);
        m_thread.quit(); m_thread.wait();
    }
    QThread *thread() { return &m_thread; }
    void setObserver(std::function<void()> observer) { QMutexLocker lock(&m_observerMutex); m_observer = std::move(observer); }

private:
    void start(quint64 id, Request request)   // worker thread
    {
        auto *job = new Job; m_jobs.insert(id, job);
        auto *process = job->process = new QProcess(m_worker);
        process->setProgram(request.program); process->setArguments(request.arguments);
        if (request.environment) process->setProcessEnvironment(*request.environment);
#if defined(Q_OS_LINUX) && QT_VERSION >= QT_VERSION_CHECK(6, 7, 0)
        // No child modifier runs here, so Linux may use vfork and skip copying
        // the page tables of the large GUI process.
        process->setUnixProcessParameters(QProcess::UnixProcessFlag::UseVFork);
#endif
        if (request.input) QObject::connect(process, &QProcess::started, process, [process, input = std::move(*request.input)]() mutable {
            if (!input.isEmpty()) process->write(input);
            input.fill('\0'); input.clear(); process->closeWriteChannel();
        });
        QObject::connect(process, &QProcess::errorOccurred, process, [this, id, job, process](QProcess::ProcessError error) {
            if (job->result.error == QProcess::UnknownError) { job->result.error = error; job->result.errorString = process->errorString(); }
            // FailedToStart is the only error not followed by finished().
            if (error == QProcess::FailedToStart) { report(id, job, Result::FailedToStart); finish(id, job); }
        });
        QObject::connect(process, &QProcess::finished, process, [this, id, job, process](int code, QProcess::ExitStatus status) {
            job->result.exitCode = code; job->result.exitStatus = status;
            job->result.standardOutput = process->readAllStandardOutput(); job->result.standardError = process->readAllStandardError();
            report(id, job, Result::Finished); finish(id, job);
        });
        if (request.timeoutMs > 0) {
            auto *timer = job->timer = new QTimer(process); timer->setSingleShot(true);
            // Report at the deadline; kill() still delivers finished() later,
            // which only reaps the child.
            QObject::connect(timer, &QTimer::timeout, process, [this, id, job, process] {
                report(id, job, Result::TimedOut); process->kill();
            });
            timer->start(request.timeoutMs);
        }
        {
            QMutexLocker lock(&m_observerMutex);
            if (m_observer) m_observer();
        }
        process->start();
    }
    void report(quint64 id, Job *job, Result::Outcome outcome)   // worker thread
    {
        if (job->reported) return;
        job->reported = true;
        Result result = outcome == Result::TimedOut ? Result{} : job->result; result.outcome = outcome;
        QMetaObject::invokeMethod(&m_front, [this, id, result = std::move(result)] { deliver(id, result); }, Qt::QueuedConnection);
    }
    void finish(quint64 id, Job *job)   // worker thread
    {
        if (m_jobs.take(id) != job) return;
        // The process is deleted later; its handlers and deadline must not see
        // the freed job meanwhile.
        if (job->timer) { job->timer->stop(); job->timer->disconnect(); }
        job->process->disconnect(); job->process->deleteLater(); delete job;
    }
    void discard(Job *job)   // worker thread
    {
        auto *process = job->process; process->disconnect();
        if (job->timer) { job->timer->stop(); job->timer->disconnect(); }
        if (process->state() != QProcess::NotRunning) { process->kill(); process->waitForFinished(1000); }
        delete process;
    }
    void deliver(quint64 id, const Result &result)   // application thread
    {
        auto it = m_pending.find(id);
        if (it == m_pending.end()) return;
        Pending pending = std::move(*it); m_pending.erase(it);
        QObject::disconnect(pending.destroyed);
        if (pending.owner) pending.done(result);
    }
    void cancel(quint64 id)   // application thread, while the owner is destroyed
    {
        auto it = m_pending.find(id);
        if (it == m_pending.end()) return;
        m_pending.erase(it);
        QMetaObject::invokeMethod(m_worker, [this, id] {
            if (auto *job = m_jobs.take(id)) { discard(job); delete job; }
        }, Qt::BlockingQueuedConnection);
    }

    QThread m_thread;
    QObject *m_worker = nullptr;
    QObject m_front;   // receives results on the application thread
    QHash<quint64, Pending> m_pending;
    QHash<quint64, Job *> m_jobs;
    quint64 m_next = 0;
    QMutex m_observerMutex;
    std::function<void()> m_observer;
};

Runner *s_runner = nullptr;
bool s_stopped = false;

void shutdown()
{
    s_stopped = true;
    auto *runner = std::exchange(s_runner, nullptr);
    if (!runner) return;
    runner->stop(); delete runner;
}

Runner *runner()
{
    if (!s_runner && !s_stopped) {
        Q_ASSERT(QCoreApplication::instance() && QThread::currentThread() == QCoreApplication::instance()->thread());
        s_runner = new Runner;
        qAddPostRoutine(shutdown);
    }
    return s_runner;
}

}

void run(Request request, QObject *owner, std::function<void(const Result &)> done)
{
    Q_ASSERT(owner && owner->thread() == QThread::currentThread());
    // After the application's post routines nothing can receive a result.
    if (auto *instance = runner()) instance->run(std::move(request), owner, std::move(done));
}

QThread *workerThread() { auto *instance = runner(); return instance ? instance->thread() : nullptr; }

void setLaunchObserver(std::function<void()> observer) { if (auto *instance = runner()) instance->setObserver(std::move(observer)); }

}
