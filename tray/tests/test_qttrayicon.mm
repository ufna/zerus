#include <QtTest>
#include <QMenu>
#include "QtTrayIcon.h"
#include "MacTrayMenu.h"
#import <AppKit/AppKit.h>

class TestQtTrayIcon : public QObject {
    Q_OBJECT
private slots:
    void primaryClickDoesNotOpenMenu();
    void nativeContextMenu();
};

void TestQtTrayIcon::primaryClickDoesNotOpenMenu()
{
    QtTrayIcon icon; QMenu menu; menu.addAction("Open hgs");
    icon.setContextMenu(&menu);
    // With a context menu attached, AppKit intercepts both mouse buttons.
    QVERIFY(!icon.item().contextMenu());
    QSignalSpy shown(&menu, &QMenu::aboutToShow);
    int activations = 0; icon.setActivationHandler([&]() { ++activations; });
    auto click = [&](QSystemTrayIcon::ActivationReason reason) {
        return QMetaObject::invokeMethod(const_cast<QSystemTrayIcon *>(&icon.item()),
            "activated", Q_ARG(QSystemTrayIcon::ActivationReason, reason));
    };
    QVERIFY(click(QSystemTrayIcon::Trigger)); QCOMPARE(activations, 1);
    QVERIFY(click(QSystemTrayIcon::DoubleClick)); QCOMPARE(activations, 2);
    QVERIFY(click(QSystemTrayIcon::MiddleClick)); QCOMPARE(activations, 2);
    QCOMPARE(shown.size(), 0);
    icon.setContextMenu(nullptr);
    QVERIFY(click(QSystemTrayIcon::Context)); QCOMPARE(activations, 2);
}

void TestQtTrayIcon::nativeContextMenu()
{
    // Opt-in: briefly shows a real native popup, then closes it without input.
    if (!qEnvironmentVariableIsSet("HGS_TEST_NATIVE_MENU"))
        QSKIP("Set HGS_TEST_NATIVE_MENU=1 with the cocoa platform for native menu integration");
    QtTrayIcon icon; QMenu menu; auto *open = menu.addAction("Open hgs");
    icon.setContextMenu(&menu);
    QSignalSpy shown(&menu, &QMenu::aboutToShow), hidden(&menu, &QMenu::aboutToHide);
    QSignalSpy triggered(open, &QAction::triggered);
    NSMenu *nativeMenu = menu.toNSMenu(); QVERIFY(nativeMenu);
    int activations = 0; icon.setActivationHandler([&]() { ++activations; });
    // NSTimer runs in AppKit's menu-tracking loop, unlike a regular Qt timer.
    NSTimer *timer = [NSTimer timerWithTimeInterval:0.15 repeats:NO block:^(NSTimer *) {
        [nativeMenu cancelTracking];
    }];
    [NSRunLoop.mainRunLoop addTimer:timer forMode:NSEventTrackingRunLoopMode];
    QVERIFY(QMetaObject::invokeMethod(const_cast<QSystemTrayIcon *>(&icon.item()),
        "activated", Q_ARG(QSystemTrayIcon::ActivationReason, QSystemTrayIcon::Context)));
    QCOMPARE(shown.size(), 1); QCOMPARE(hidden.size(), 1); QCOMPARE(activations, 0);
    [nativeMenu performActionForItemAtIndex:0];
    QTRY_COMPARE(triggered.size(), 1);
}

QTEST_MAIN(TestQtTrayIcon)
#include "test_qttrayicon.moc"
