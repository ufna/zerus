#include "MainThreadWatchdog.h"

#include <QObject>
#include <QtGlobal>

#include <algorithm>
#include <pthread.h>

std::int64_t MainThreadWatchdog::nowMs()
{
    return std::chrono::duration_cast<std::chrono::milliseconds>(
               Clock::now().time_since_epoch())
        .count();
}

MainThreadWatchdog::MainThreadWatchdog(int thresholdMs, std::function<void()> onHang)
    : m_thresholdMs(thresholdMs)
    , m_onHang(std::move(onHang))
    , m_lastBeatMs(nowMs())
{
    m_heartbeat.setInterval(std::max(1, thresholdMs / 4));
    QObject::connect(&m_heartbeat, &QTimer::timeout, &m_heartbeat,
                     [this]() { m_lastBeatMs.store(nowMs()); });
    m_heartbeat.start();

    m_thread = std::thread([this]() {
#if defined(Q_OS_MACOS)
        pthread_setname_np("hgs-watchdog");
#elif defined(Q_OS_LINUX)
        pthread_setname_np(pthread_self(), "hgs-watchdog");
#endif
        const auto period = std::chrono::milliseconds(std::max(1, m_thresholdMs / 2));
        std::unique_lock<std::mutex> lock(m_mutex);
        // wait_for возвращает true только по m_stop; таймаут -- очередная проверка.
        while (!m_wake.wait_for(lock, period, [this]() { return m_stop; })) {
            if (nowMs() - m_lastBeatMs.load() <= m_thresholdMs)
                continue;
            lock.unlock();
            m_onHang();
            return;   // не больше одного раза
        }
    });
}

MainThreadWatchdog::~MainThreadWatchdog()
{
    {
        std::lock_guard<std::mutex> lock(m_mutex);
        m_stop = true;
    }
    m_wake.notify_all();
    if (m_thread.joinable())
        m_thread.join();
}
