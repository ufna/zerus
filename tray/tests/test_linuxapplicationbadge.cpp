#include <QtTest>
#include <QDBusMessage>
#include <QDBusPendingReply>
#include "LinuxApplicationBadge.h"

class BadgeReceiver : public QObject {
    Q_OBJECT
public slots:
    void update(const QString &app, const QVariantMap &properties) { emit received(app, properties); }
signals:
    void received(const QString &app, const QVariantMap &properties);
};

class TestLinuxApplicationBadge : public QObject {
    Q_OBJECT
private slots:
    void countTransitionsQueryShellRestartAndShutdown() {
        // CTest runs this process under dbus-run-session. Never publish test
        // counts on the user's desktop session bus.
        auto publisher = QDBusConnection::connectToBus(QDBusConnection::SessionBus, "badge-publisher");
        auto shell = QDBusConnection::connectToBus(QDBusConnection::SessionBus, "badge-shell");
        BadgeReceiver receiver; QSignalSpy updates(&receiver, &BadgeReceiver::received);
        QVERIFY(shell.connect({}, "/org/hgdev/Zerus/LauncherEntry", "com.canonical.Unity.LauncherEntry", "Update",
            &receiver, SLOT(update(QString,QVariantMap))));
        auto badge = std::make_unique<LinuxApplicationBadge>(publisher);
        badge->setCount(3); QTRY_COMPARE(updates.size(), 1);
        QCOMPARE(updates[0][0].toString(), QString("application://hgs-tray.desktop"));
        const auto properties = updates[0][1].toMap();
        QCOMPARE(properties["count"].metaType().id(), QMetaType::LongLong);
        QCOMPARE(properties["count"].toLongLong(), qint64(3)); QVERIFY(properties["count-visible"].toBool());
        badge->setCount(3); QTest::qWait(30); QCOMPARE(updates.size(), 1);
        badge->setCount(123); QTRY_COMPARE(updates.size(), 2);
        QCOMPARE(updates.last()[1].toMap()["count"].toLongLong(), qint64(123));
        auto query = QDBusMessage::createMethodCall(publisher.baseService(), "/org/hgdev/Zerus/LauncherEntry", "com.canonical.Unity.LauncherEntry", "Query");
        QDBusPendingReply<QString, QVariantMap> reply = shell.asyncCall(query);
        QTRY_VERIFY(reply.isFinished()); QVERIFY2(!reply.isError(), qPrintable(reply.error().message()));
        QCOMPARE(reply.argumentAt<0>(), QString("application://hgs-tray.desktop"));
        QCOMPARE(reply.argumentAt<1>()["count"].toLongLong(), qint64(123));
        QVERIFY(shell.registerService("org.kde.plasmashell"));
        QTRY_COMPARE(updates.size(), 3); QCOMPARE(updates.last()[1].toMap()["count"].toLongLong(), qint64(123));
        QVERIFY(shell.unregisterService("org.kde.plasmashell"));
        QVERIFY(shell.registerService("org.kde.plasmashell"));
        QTRY_COMPARE(updates.size(), 4); QCOMPARE(updates.last()[1].toMap()["count"].toLongLong(), qint64(123));
        badge->setCount(0); QTRY_COMPARE(updates.size(), 5); QVERIFY(!updates.last()[1].toMap()["count-visible"].toBool());
        badge->setCount(-1); QTest::qWait(30); QCOMPARE(updates.size(), 5);
        badge->setCount(1); QTRY_COMPARE(updates.size(), 6);
        badge.reset(); QTRY_COMPARE(updates.size(), 7); QVERIFY(!updates.last()[1].toMap()["count-visible"].toBool());
    }
};
QTEST_GUILESS_MAIN(TestLinuxApplicationBadge)
#include "test_linuxapplicationbadge.moc"
