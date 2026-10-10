#include <QtTest>
#include "SwarmButton.h"

class TestSwarmButton : public QObject {
    Q_OBJECT
private slots:
    void attentionAnimationUsesBoundedFrameRate();
    void animationStopsWhenHiddenOrReduced();
    void unchangedAppearanceDoesNotRepaint();
};

namespace {
class PaintCounter : public QObject {
public:
    int paints = 0;
protected:
    bool eventFilter(QObject *, QEvent *event) override { if (event->type() == QEvent::Paint) ++paints; return false; }
};
QTimer *frameTimer(SwarmButton &button) { return button.findChild<QTimer *>("swarmFrameTimer"); }
}

void TestSwarmButton::attentionAnimationUsesBoundedFrameRate()
{
    SwarmButton button; button.resize(42, 44); button.show(); QVERIFY(QTest::qWaitForWindowExposed(&button));
    button.setAppearance(Qt::white, QColor("#ffda76"), true, false);
    auto *timer = frameTimer(button); QVERIFY(timer); QVERIFY(timer->isActive());
    // The mark moves during the first 1.4 s of each 3 s cycle; frames stay at 30 fps whatever the display rate.
    QCOMPARE(timer->interval(), SwarmButton::kFrameMs);
    QVERIFY(SwarmButton::kFrameMs >= 33);
    QTest::qWait(120); QVERIFY(timer->isActive()); QVERIFY(timer->interval() >= SwarmButton::kFrameMs);
}

void TestSwarmButton::animationStopsWhenHiddenOrReduced()
{
    SwarmButton button; button.resize(42, 44); button.show(); QVERIFY(QTest::qWaitForWindowExposed(&button));
    button.setAppearance(Qt::white, QColor("#ffda76"), true, false);
    auto *timer = frameTimer(button); QVERIFY(timer->isActive());
    button.hide(); QVERIFY(!timer->isActive());
    button.show(); QVERIFY(timer->isActive());
    button.setAppearance(Qt::white, QColor("#ffda76"), true, true); QVERIFY(!timer->isActive());
    button.setAppearance(Qt::white, QColor("#ffda76"), false, false); QVERIFY(!timer->isActive());
}

void TestSwarmButton::unchangedAppearanceDoesNotRepaint()
{
    SwarmButton button; button.resize(42, 44);
    PaintCounter counter; button.installEventFilter(&counter);
    button.show(); QVERIFY(QTest::qWaitForWindowExposed(&button));
    button.setAppearance(QColor("#237a62"), QColor("#a66000"), false, false); QTest::qWait(30);
    counter.paints = 0;
    // Polls refresh the attention state every few seconds; an unchanged state must not redraw the rail.
    for (int i = 0; i < 3; ++i) button.setAppearance(QColor("#237a62"), QColor("#a66000"), false, false);
    QTest::qWait(30); QCOMPARE(counter.paints, 0);
    button.setAppearance(QColor("#237a62"), QColor("#a66000"), true, true); QTest::qWait(30);
    QVERIFY(counter.paints > 0);
}

QTEST_MAIN(TestSwarmButton)
#include "test_swarmbutton.moc"
