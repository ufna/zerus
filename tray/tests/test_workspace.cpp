#include <QtTest>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QMimeData>
#include <QLineEdit>
#include <QVBoxLayout>
#include "SessionOrganization.h"
#include "SessionList.h"
#include "SessionCardDelegate.h"
#include "SessionElapsed.h"
#include "IdentityBadge.h"

class ListProbe : public SessionList { public: using SessionList::mimeData; using SessionList::initViewItemOption; };
class TestWorkspace : public QObject {
    Q_OBJECT
private slots:
    void durableIdentityAndMissingSessions();
    void orderingAndRemoval();
    void invalidDataAndMoves();
    void nativeDropPlacement();
    void pinningRestoresPreviousGroup();
    void projectsShareFoldersButOwnSessions();
    void defaultProjectCanBeRenamedAndReplaced();
    void keyboardFocusFollowsInput();
    void activityAnimationOnlyRunsForVisibleWorkingRows();
    void elapsedWorkTimeKeepsUpdatingWithoutAnimation();
    void shortIdentityLabelsFitWithoutElision();
    void draftStatusSitsBetweenWorkAndReplies();
};

void TestWorkspace::shortIdentityLabelsFitWithoutElision()
{
    // Exercise native metrics and fractional advances even on platforms whose
    // default font happens to give these aliases integral widths.
    for (const qreal spacing : {0.0, 0.1, 0.2, 0.3}) {
        QFont base = QApplication::font(); base.setLetterSpacing(QFont::AbsoluteSpacing, spacing);
        for (const QString &name : {QString("mac"), QString("arch"), QString("studio-mac")}) {
            IdentityBadge badge(IdentityBadges::Machine); badge.setFont(base); badge.setValue(name);
            const QFontMetrics metrics(IdentityBadges::font(base, IdentityBadges::Machine));
            QCOMPARE(metrics.elidedText(name, Qt::ElideMiddle, badge.sizeHint().width() - 12), name);
        }
        IdentityBadge longName(IdentityBadges::Machine); longName.setFont(base);
        longName.setValue("a-very-long-machine-name-that-must-still-fit-in-the-layout");
        QVERIFY(longName.sizeHint().width() <= 140);
    }
}

void TestWorkspace::elapsedWorkTimeKeepsUpdatingWithoutAnimation()
{
    QCOMPARE(SessionElapsed::working(100, 1176), QString("Working (17m 56s)"));
    QCOMPARE(SessionElapsed::text(100, 118), QString("18s"));
    QCOMPARE(SessionElapsed::text(100, 3760), QString("1h 01m"));
    QCOMPARE(SessionElapsed::text(100, 176500), QString("2d 1h"));
    QCOMPARE(SessionElapsed::text(100, 99), QString("0s"));
    QCOMPARE(SessionElapsed::working(0, 100), QString("Working"));
    QCOMPARE(SessionElapsed::status("Compacting", 100, 183), QString("Compacting (1m 23s)"));
    SessionList list; list.resize(320, 240); list.setActivityAnimationEnabled(false);
    auto *busy = new QListWidgetItem("Working", &list);
    busy->setData(SessionRoles::Working, true); busy->setData(SessionRoles::WorkingSince, 100.0);
    list.show(); QTRY_VERIFY(list.elapsedTimerRunning()); QVERIFY(!list.activityAnimationRunning());
    busy->setData(SessionRoles::Working, false); list.syncActivityAnimation(); QVERIFY(!list.elapsedTimerRunning());
    busy->setData(SessionRoles::Working, true); list.syncActivityAnimation(); QVERIFY(list.elapsedTimerRunning());
    busy->setHidden(true); list.syncActivityAnimation(); QVERIFY(!list.elapsedTimerRunning());
    busy->setHidden(false); list.syncActivityAnimation(); QVERIFY(list.elapsedTimerRunning());
    list.hide(); QVERIFY(!list.elapsedTimerRunning());
}

void TestWorkspace::activityAnimationOnlyRunsForVisibleWorkingRows()
{
    SessionList list; list.resize(320, 240);
    auto *busy = new QListWidgetItem("Working", &list); busy->setData(SessionRoles::Working, 1);
    list.syncActivityAnimation(); QVERIFY(!list.activityAnimationRunning());
    list.show(); QTRY_VERIFY(list.activityAnimationRunning());
    QTRY_VERIFY(list.property("workingPulse").toReal() > .05);
    list.setActivityAnimationEnabled(false); QVERIFY(!list.activityAnimationRunning());
    list.setActivityAnimationEnabled(true); QVERIFY(list.activityAnimationRunning());
    busy->setHidden(true); list.syncActivityAnimation(); QVERIFY(!list.activityAnimationRunning());
    auto *group = new QListWidgetItem("Collapsed group", &list); group->setData(SessionRoles::Header, true); group->setData(SessionRoles::Working, 1);
    list.syncActivityAnimation(); QVERIFY(list.activityAnimationRunning());
    group->setData(SessionRoles::Working, 0); list.syncActivityAnimation(); QVERIFY(!list.activityAnimationRunning());
    busy->setHidden(false); list.syncActivityAnimation(); QVERIFY(list.activityAnimationRunning());
    list.hide(); QVERIFY(!list.activityAnimationRunning());
}

void TestWorkspace::durableIdentityAndMissingSessions()
{
    SessionOrganization model;
    const auto group = model.createGroup("Release");
    const auto arch = model.observe("arch", "codex/project/review", "run-1");
    const auto mac = model.observe("mac", "codex/project/review", "run-1");
    model.moveSession(arch, group); model.moveSession(mac, group); model.setCollapsed(group, true);
    // Reload with one machine offline: its membership and position are retained.
    SessionOrganization reloaded(model.toJson());
    QCOMPARE(reloaded.observe("arch", "codex/project/review", "run-2"), arch);
    QCOMPARE(reloaded.group(group)->sessions, QStringList({arch, mac}));
    QVERIFY(reloaded.group(group)->collapsed);
    const auto renamed = reloaded.observe("arch", "codex/project/release", "run-2");
    QCOMPARE(reloaded.groupFor(renamed), group);
    QCOMPARE(reloaded.group(group)->sessions, QStringList({renamed, mac}));
    // Archive occurrences remain distinct even with identical names and run IDs.
    const auto archive = reloaded.observe("arch", "codex/project/release", "run-2", "archive-1");
    QVERIFY(archive != renamed); QCOMPARE(reloaded.groupFor(archive), QString("ungrouped"));
    QCOMPARE(reloaded.observe("arch", "codex/project/renamed-archive", "run-2", "archive-1"), archive);
    QCOMPARE(SessionOrganization(reloaded.toJson()).toJson(), reloaded.toJson());
}

void TestWorkspace::orderingAndRemoval()
{
    SessionOrganization model;
    const auto first = model.createGroup("First"), second = model.createGroup("Second");
    for (const auto &id : {"a", "hidden", "b", "c"}) model.moveSession(id, first);
    model.moveSession("c", first, "b"); // Moving a visible item doesn't drop filtered members.
    QCOMPARE(model.group(first)->sessions, QStringList({"a", "hidden", "c", "b"}));
    model.moveSession("a", second); model.moveSession("c", second, "a");
    QCOMPARE(model.group(second)->sessions, QStringList({"c", "a"}));
    model.moveGroup(second, first); QCOMPARE(model.groups().at(0).id, second);
    model.moveGroup("ungrouped", second); QCOMPARE(model.groups().at(0).id, QString("ungrouped"));
    model.removeGroup(second); QVERIFY(!model.group(second));
    QCOMPARE(model.group("ungrouped")->sessions, QStringList({"c", "a"}));
    model.renameGroup(first, "  New name  "); QCOMPARE(model.group(first)->name, QString("New name"));
    model.removeGroup("ungrouped"); QVERIFY(model.group("ungrouped"));
}

void TestWorkspace::invalidDataAndMoves()
{
    SessionOrganization model(QJsonObject{{"groups", QJsonArray{
        QJsonObject{{"id", "one"}, {"name", "One"}, {"sessions", QJsonArray{"a", "a", ""}}},
        QJsonObject{{"id", "one"}, {"name", "Duplicate"}},
        QJsonObject{{"id", "two"}, {"name", "Two"}, {"sessions", QJsonArray{"a", "b"}}}}}});
    QCOMPARE(model.groups().size(), 3); QCOMPARE(model.group("one")->sessions, QStringList{"a"});
    QCOMPARE(model.group("two")->sessions, QStringList{"b"});
    const auto before = model.toJson();
    model.moveSession("a", "missing"); model.moveSession("a", "two", "missing"); model.moveSession("a", "one", "a");
    model.moveGroup("one", "missing"); model.moveGroup("one", "one"); model.createGroup(" ");
    QCOMPARE(model.toJson(), before);
}

void TestWorkspace::nativeDropPlacement()
{
    ListProbe list; list.resize(310, 450);
    const auto add = [&](const QString &group, const QString &id, bool header = false) {
        auto *item = new QListWidgetItem(header ? group : id, &list); item->setSizeHint(QSize(300, header ? 34 : 68));
        item->setData(SessionRoles::Header, header); item->setData(SessionRoles::Group, group); item->setData(SessionRoles::Identity, id);
        item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsDragEnabled | Qt::ItemIsDropEnabled); return item;
    };
    auto *first = add("first", {}, true); auto *a = add("first", "a"); auto *b = add("first", "b");
    auto *second = add("second", {}, true); auto *c = add("second", "c"); list.show(); QTest::qWait(20);
    QSignalSpy moved(&list, &SessionList::sessionMoved), reordered(&list, &SessionList::groupMoved);
    const auto drop = [&](QListWidgetItem *source, QPoint point) {
        std::unique_ptr<QMimeData> mime(list.mimeData({source}));
        QDragEnterEvent enter(point, Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(list.viewport(), &enter); QVERIFY(enter.isAccepted());
        QDragMoveEvent motion(point, Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(list.viewport(), &motion); QVERIFY(motion.isAccepted());
        QDropEvent event(point, Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(list.viewport(), &event); QVERIFY(event.isAccepted());
    };
    drop(a, list.visualItemRect(second).center()); // Can drop into an empty or collapsed group.
    QCOMPARE(moved.takeFirst(), QList<QVariant>({"a", "second", ""}));
    drop(c, list.visualItemRect(b).topLeft() + QPoint(50, 3));
    QCOMPARE(moved.takeFirst(), QList<QVariant>({"c", "first", "b"}));
    drop(second, list.visualItemRect(first).topLeft() + QPoint(50, 3));
    QCOMPARE(reordered.takeFirst(), QList<QVariant>({"second", "first"}));
    drop(first, QPoint(100, 430)); QCOMPARE(reordered.takeFirst(), QList<QVariant>({"first", ""}));
    auto *child = new QListWidgetItem("Child");
    child->setData(SessionRoles::ChildId, "child"); child->setData(SessionRoles::ParentKey, a->data(SessionRoles::Key));
    child->setData(SessionRoles::Group, "first"); child->setSizeHint(QSize(280, 40));
    list.insertItem(list.row(a) + 1, child);
    std::unique_ptr<QMimeData> childMime(list.mimeData({child}));
    QVERIFY(!childMime->hasFormat("application/x-hgs-session-placement"));
    drop(c, list.visualItemRect(child).bottomLeft() + QPoint(50, -2));
    QCOMPARE(moved.takeFirst(), QList<QVariant>({"c", "first", "b"}));
    delete list.takeItem(list.row(child));
    // Payloads from another window/application are rejected.
    ListProbe foreign; auto *other = new QListWidgetItem("foreign", &foreign); other->setData(SessionRoles::Identity, "a");
    std::unique_ptr<QMimeData> mime(foreign.mimeData({other}));
    QDragEnterEvent invalid(QPoint(20, 20), Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(list.viewport(), &invalid); QVERIFY(!invalid.isAccepted());
    QSignalSpy toggled(&list, &SessionList::groupToggled);
    list.setCurrentItem(a); QTest::mouseClick(list.viewport(), Qt::LeftButton, Qt::NoModifier, list.visualItemRect(first).center());
    QCOMPARE(toggled.size(), 1); QCOMPARE(list.currentItem(), a);
}

void TestWorkspace::pinningRestoresPreviousGroup()
{
    const QJsonObject legacy{{"version",1},{"groups",QJsonArray{
        QJsonObject{{"id","pinned"},{"name","Pinned"},{"sessions",QJsonArray{"one","orphan"}}},
        QJsonObject{{"id","work"},{"name","Work"},{"collapsed",true},{"sessions",QJsonArray{"two"}}}}},
        {"before_pin",QJsonObject{{"one","work"},{"orphan","removed"}}}};
    SessionOrganization model(legacy);
    QVERIFY(!model.group("pinned")); QCOMPARE(model.groupFor("one"),QString("work"));
    QCOMPARE(model.groupFor("orphan"),QString("ungrouped")); QVERIFY(model.group("work")->collapsed);
    QVERIFY(model.toJson().contains("projects")); QVERIFY(!model.toJson().contains("groups"));
    QCOMPARE(SessionOrganization(model.toJson()).toJson(), model.toJson());
}

void TestWorkspace::defaultProjectCanBeRenamedAndReplaced()
{
    SessionOrganization model;const auto initial=model.defaultProject();
    model.renameGroup(initial,"My workspace");QCOMPARE(model.group(initial)->name,QString("My workspace"));
    model.observe("arch","session","run",{});const auto target=model.createGroup("Product");
    QVERIFY(!model.removeGroup(initial));QVERIFY(!model.removeGroup(initial,"missing"));
    QVERIFY(model.removeGroup(initial,target));QCOMPARE(model.defaultProject(),target);
    QCOMPARE(model.groupFor("arch\nsession"),target);QVERIFY(!model.group(initial));
    SessionOrganization restored(model.toJson());QCOMPARE(restored.groups().size(),1);
    QCOMPARE(restored.defaultProject(),target);restored.observe("mac","new","another",{});
    QCOMPARE(restored.groupFor("mac\nnew"),target);QVERIFY(!restored.removeGroup(target));
}

void TestWorkspace::projectsShareFoldersButOwnSessions()
{
    SessionOrganization model; const auto a=model.createGroup("Product / alpha"),b=model.createGroup("Customer beta");
    const auto one=model.addFolder(a,"arch","/workspace/shared","Backend");
    QVERIFY(!one.isEmpty()); QCOMPARE(model.addFolder(a,"arch","/workspace/shared/"),one);
    QVERIFY(!model.addFolder(b,"arch","/workspace/shared").isEmpty());
    QVERIFY(!model.addFolder(a,"mac","/Users/test/shared").isEmpty());
    model.setColor(a,"#ffcc00"); model.moveSession("arch\nsession",a); model.moveSession("arch\nsession",b);
    QVERIFY(!model.group(a)->sessions.contains("arch\nsession")); QCOMPARE(model.groupFor("arch\nsession"),b);
    QCOMPARE(model.group(a)->folders.size(),2); QCOMPARE(model.group(b)->folders.size(),1);
    QVERIFY(model.editFolder(a,one,"arch","/workspace/renamed","New folder"));
    QCOMPARE(model.group(a)->folders.first().id,one);
    QCOMPARE(model.group(b)->folders.first().path,QString("/workspace/shared"));
    model.renameGroup(a,"Renamed"); QCOMPARE(model.group(a)->color,QString("#ffcc00"));
    SessionOrganization restored(model.toJson()); QCOMPARE(restored.group(a)->folders[0].id,one);
    restored.removeGroup(b); QCOMPARE(restored.groupFor("arch\nsession"),QString("ungrouped"));
    QCOMPARE(restored.group(a)->folders.size(),2);
}

void TestWorkspace::keyboardFocusFollowsInput()
{
    QWidget window; QVBoxLayout layout(&window); QLineEdit input; ListProbe list;
    layout.addWidget(&input); layout.addWidget(&list); list.addItems({"First", "Second"}); window.show(); window.activateWindow(); QTest::qWait(20);
    const auto keyboard = [&] { QStyleOptionViewItem option; list.initViewItemOption(&option); return bool(option.state & QStyle::State_KeyboardFocusChange); };
    QTest::mouseClick(list.viewport(), Qt::LeftButton, Qt::NoModifier, list.visualItemRect(list.item(0)).center());
    QCOMPARE(list.currentRow(), 0); QVERIFY(list.hasFocus()); QVERIFY(!keyboard());
    QTest::keyClick(&list, Qt::Key_Down); QCOMPARE(list.currentRow(), 1); QVERIFY(keyboard());
    QTest::mouseClick(list.viewport(), Qt::LeftButton, Qt::NoModifier, list.visualItemRect(list.item(1)).center());
    QCOMPARE(list.currentRow(), 1); QVERIFY(!keyboard());
    input.setFocus(); QTest::keyClick(&input, Qt::Key_Tab); QVERIFY(list.hasFocus()); QVERIFY(keyboard());
    QTest::mouseClick(list.viewport(), Qt::RightButton, Qt::NoModifier, list.visualItemRect(list.item(1)).center());
    QVERIFY(!keyboard()); QCOMPARE(list.currentRow(), 1);
}

void TestWorkspace::draftStatusSitsBetweenWorkAndReplies()
{
    // An unsent draft is a status of its own: below anything asking for the user or
    // still working, above a new reply (kept as the extra badge), a pause and Ready.
    QListWidget list; auto *item = new QListWidgetItem("session", &list);
    const auto status = [&](bool selected = false) { return SessionDelegate::statusOf(list.model()->index(0, 0), selected); };
    item->setData(SessionRoles::Status, "Ready"); item->setData(SessionRoles::Draft, true);
    QCOMPARE(status().kind, SessionStatusBadge::Draft); QCOMPARE(status().caption, QString("Draft"));
    QCOMPARE(status(true).kind, SessionStatusBadge::Neutral);   // the open session shows its draft in place
    item->setData(SessionRoles::Unread, true);
    QCOMPARE(status().kind, SessionStatusBadge::Draft); QVERIFY(status().additionalUnread);
    item->setData(SessionRoles::Working, 1);
    QCOMPARE(status().kind, SessionStatusBadge::Working);
    item->setData(SessionRoles::Working, 0); item->setData(SessionRoles::Attention, true);
    QCOMPARE(status().kind, SessionStatusBadge::Attention);
    item->setData(SessionRoles::Attention, false); item->setData(SessionRoles::Unread, false); item->setData(SessionRoles::Status, "Paused");
    QCOMPARE(status().kind, SessionStatusBadge::Draft);
    item->setData(SessionRoles::Draft, false);
    QCOMPARE(status().kind, SessionStatusBadge::Paused);
    item->setData(SessionRoles::Draft, true); item->setData(SessionRoles::Status, "Offline");
    QCOMPARE(status().kind, SessionStatusBadge::Neutral); QCOMPARE(status().caption, QString("Offline"));
}

QTEST_MAIN(TestWorkspace)
#include "test_workspace.moc"
