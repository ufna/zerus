#include <QtTest>
#include "ProjectsDialog.h"
#include <QInputDialog>
#include <QListWidget>
#include <QMenu>
#include <QSettings>
#include <QTemporaryDir>

class TestProjectsDialog : public QObject {
    Q_OBJECT
private slots:
    void emptyListContextCreatesProject();
    void acceptsPlainName();
    void rejectsSlashAndSpace();
    void rejectsEmpty();
    void warnsOnDotOrColon();
    void nameDefaultsToBasename();
};

void TestProjectsDialog::emptyListContextCreatesProject()
{
    QTemporaryDir directory;QVERIFY(directory.isValid());
    QSettings::setDefaultFormat(QSettings::IniFormat);QSettings::setPath(QSettings::IniFormat,QSettings::UserScope,directory.path());
    QCoreApplication::setOrganizationName("hgs-project-menu-test");QCoreApplication::setApplicationName("projects");
    HgsClient client("/nonexistent/hgs");SessionOrganization projects;
    ProjectsDialog dialog(&client,nullptr,false,&projects);dialog.show();QTest::qWait(30);
    auto *list=dialog.findChild<QListWidget*>("logicalProjects");QVERIFY(list);
    const auto selected=list->currentItem();const auto empty=list->viewport()->rect().bottomLeft()+QPoint(5,-5);QVERIFY(!list->itemAt(empty));
    list->customContextMenuRequested(empty);
    auto *menu=dialog.findChild<QMenu*>("projectListMenu");QVERIFY(menu);QCOMPARE(menu->actions().size(),1);QCOMPARE(list->currentItem(),selected);
    auto *add=menu->actions().first();QCOMPARE(add->objectName(),"addProjectAction");menu->hide();
    QTimer::singleShot(0,&dialog,[]{auto *input=qobject_cast<QInputDialog*>(QApplication::activeModalWidget());QVERIFY(input);input->setTextValue("Created from empty space");input->accept();});
    add->trigger();
    QVERIFY(std::any_of(projects.groups().cbegin(),projects.groups().cend(),[](const auto &p){return p.name=="Created from empty space";}));
    auto *item=list->currentItem();QVERIFY(item);list->customContextMenuRequested(list->visualItemRect(item).center());
    menu=qobject_cast<QMenu*>(QApplication::activePopupWidget());QVERIFY(menu);QCOMPARE(menu->actions().first()->objectName(),"addProjectAction");
    QVERIFY(menu->findChild<QAction*>("addProjectAction"));QCOMPARE(menu->actions().size(),4);menu->close();
}

void TestProjectsDialog::acceptsPlainName()
{
    QString why;
    QVERIFY(ProjectsDialog::nameProblem(QStringLiteral("sample-project"), &why) == false);
    QVERIFY(why.isEmpty());
}

void TestProjectsDialog::rejectsSlashAndSpace()
{
    QString why;
    QVERIFY(!ProjectsDialog::nameProblem(QStringLiteral("a/b"), &why));
    QVERIFY(why.isEmpty());
    QVERIFY(!ProjectsDialog::nameProblem(QStringLiteral("a b"), &why));
}

void TestProjectsDialog::rejectsEmpty()
{
    QString why;
    QVERIFY(ProjectsDialog::nameProblem(QString(), &why));
}

void TestProjectsDialog::warnsOnDotOrColon()
{
    // Точка допустима (example.com — пример с точкой), но в имени сессии станет "_".
    QString why;
    QVERIFY(!ProjectsDialog::nameProblem(QStringLiteral("example.com"), &why));
    QVERIFY(ProjectsDialog::nameWarning(QStringLiteral("example.com")).isEmpty());
    QVERIFY(ProjectsDialog::nameWarning(QStringLiteral("plain")).isEmpty());
}

void TestProjectsDialog::nameDefaultsToBasename()
{
    QCOMPARE(ProjectsDialog::suggestName(QStringLiteral("/workspace/sample-tray")),
             QStringLiteral("sample-tray"));
    QCOMPARE(ProjectsDialog::suggestName(QStringLiteral("/workspace/sample-tray/")),
             QStringLiteral("sample-tray"));
}

QTEST_MAIN(TestProjectsDialog)
#include "test_projectsdialog.moc"
