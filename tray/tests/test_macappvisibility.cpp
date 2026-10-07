#include "MacAppVisibility.h"

#include <QApplication>
#include <QCloseEvent>
#include <QDialog>
#include <QEvent>
#include <QMenu>
#include <QTest>
#include <QWidget>

class TestMacAppVisibility : public QObject {
    Q_OBJECT
private slots:
    void followsOpenWindows();
    void minimizesAndReopens();
    void ignoresTransientUi();
    void rejectedCloseAndDestruction();
};

void TestMacAppVisibility::followsOpenWindows()
{
    QList<bool> policies;
    MacAppVisibility visibility(*qApp, [&](bool regular) { policies << regular; });
    QWidget window;
    QCoreApplication::processEvents();
    QVERIFY(policies.isEmpty());
    window.show();
    QTRY_COMPARE(policies, QList<bool>{true});
    QDialog dialog;
    dialog.show();
    window.close();
    QCoreApplication::processEvents();
    QCOMPARE(policies, QList<bool>{true});
    dialog.close();
    QTRY_COMPARE(policies, (QList<bool>{true, false}));
    window.show();
    QTRY_COMPARE(policies, (QList<bool>{true, false, true}));
    window.close();
    QTRY_COMPARE(policies, (QList<bool>{true, false, true, false}));
}

void TestMacAppVisibility::minimizesAndReopens()
{
    QList<bool> policies;
    MacAppVisibility visibility(*qApp, [&](bool regular) { policies << regular; });
    QWidget window;
    window.show();
    QTRY_COMPARE(policies, QList<bool>{true});
    window.showMinimized();
    QCoreApplication::processEvents();
    QVERIFY(window.isMinimized());
    QCOMPARE(policies, QList<bool>{true});
    QEvent reopen(QEvent::ApplicationActivate);
    QCoreApplication::sendEvent(qApp, &reopen);
    QTRY_VERIFY(!window.isMinimized());
    QCOMPARE(policies, QList<bool>{true});
    window.showMinimized();
    window.close();
    QTRY_COMPARE(policies, (QList<bool>{true, false}));
}

void TestMacAppVisibility::ignoresTransientUi()
{
    QList<bool> policies;
    MacAppVisibility visibility(*qApp, [&](bool regular) { policies << regular; });
    QWidget popup(nullptr, Qt::Popup), tooltip(nullptr, Qt::ToolTip), tool(nullptr, Qt::Tool);
    QMenu menu;
    popup.show(); tooltip.show(); tool.show(); menu.show();
    QCoreApplication::processEvents();
    QVERIFY(policies.isEmpty());
    QWidget window;
    window.show();
    QTRY_COMPARE(policies, QList<bool>{true});
    window.close();
    QTRY_COMPARE(policies, (QList<bool>{true, false}));
}

void TestMacAppVisibility::rejectedCloseAndDestruction()
{
    class PersistentWindow : public QWidget {
        void closeEvent(QCloseEvent *event) override { event->ignore(); }
    };
    QList<bool> policies;
    MacAppVisibility visibility(*qApp, [&](bool regular) { policies << regular; });
    auto *window = new PersistentWindow;
    window->show();
    QTRY_COMPARE(policies, QList<bool>{true});
    QVERIFY(!window->close());
    QCoreApplication::processEvents();
    QCOMPARE(policies, QList<bool>{true});
    delete window;
    QTRY_COMPARE(policies, (QList<bool>{true, false}));
}

QTEST_MAIN(TestMacAppVisibility)
#include "test_macappvisibility.moc"
