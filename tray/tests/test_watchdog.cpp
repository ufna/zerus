#include <QtTest>

#include <QThread>

#include <atomic>

#include "MainThreadWatchdog.h"

// Сторож главного потока (см. MainThreadWatchdog.h). Тест сам и есть главный поток:
// «зависание» -- это QThread::msleep без цикла событий, «здоровье» -- QTest::qWait,
// который цикл крутит. Колбэк приходит ИЗ СТОРОЖЕВОГО потока, отсюда atomic.
// Порог 300 мс против секунды простоя -- запас на загруженный бокс, а не точность.

class TestMainThreadWatchdog : public QObject {
    Q_OBJECT
private slots:
    void firesOnceWhenTheMainThreadStopsPumpingEvents();
    void staysQuietWhileEventsFlow();
    void stopsWatchingWhenDestroyed();
};

void TestMainThreadWatchdog::firesOnceWhenTheMainThreadStopsPumpingEvents()
{
    std::atomic<int> fired{0};
    MainThreadWatchdog dog(300, [&fired]() { ++fired; });
    QThread::msleep(1000);
    // Ровно один раз, а не на каждой проверке: в бою колбэк -- abort(), второго шанса
    // ему не нужно, а в тесте повторные срабатывания маскировали бы гонки.
    QCOMPARE(fired.load(), 1);
}

void TestMainThreadWatchdog::staysQuietWhileEventsFlow()
{
    std::atomic<int> fired{0};
    MainThreadWatchdog dog(300, [&fired]() { ++fired; });
    QTest::qWait(1000);
    QCOMPARE(fired.load(), 0);
}

void TestMainThreadWatchdog::stopsWatchingWhenDestroyed()
{
    std::atomic<int> fired{0};
    {
        MainThreadWatchdog dog(300, [&fired]() { ++fired; });
    }
    QThread::msleep(800);
    QCOMPARE(fired.load(), 0);
}

QTEST_GUILESS_MAIN(TestMainThreadWatchdog)
#include "test_watchdog.moc"
