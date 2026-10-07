#include <QtTest>
#include <QTemporaryDir>
#include <QFile>
#include <QSettings>
#include <QComboBox>
#include <QLineEdit>
#include <QTimer>
#include "WorktreePanel.h"
#include "NewSessionDialog.h"

class TestWorktreePanel:public QObject {
    Q_OBJECT
    QTemporaryDir dir;
    QString script() const {return dir.filePath("hgs");}
    QJsonObject catalog() const {
        return {{"state","ok"},{"machine","fixture"},{"path","/repo"},{"selected_root","/repo"},{"common_dir","/repo/.git"},{"sampled_at",QDateTime::currentSecsSinceEpoch()},
            {"worktrees",QJsonArray{QJsonObject{{"path","/repo"},{"kind","main"},{"branch","main"},{"available",true}},QJsonObject{{"path","/linked"},{"kind","linked"},{"branch","feature"},{"available",true}},QJsonObject{{"path","/missing"},{"kind","linked"},{"prunable",true},{"available",false}},QJsonObject{{"path","/bare"},{"kind","bare"},{"available",true}}}}};
    }
    FleetState fleet() const {
        BoxState local;local.ok=true;local.host="arch";SessionInfo a;a.name="codex/repo/one";a.gitRoot="/repo";a.canonicalCwd="/repo/subfolder";local.sessions={a};
        BoxState remote=local;remote.host="mac";remote.sessions[0].name="kimi/repo/two";
        FleetState result;result.setLocal(local,QDateTime::currentMSecsSinceEpoch());result.setPeer(remote,QDateTime::currentMSecsSinceEpoch());return result;
    }
private slots:
    void initTestCase(){
        QCoreApplication::setOrganizationName("hgs-tests");QCoreApplication::setApplicationName("worktree-panel");QSettings::setDefaultFormat(QSettings::IniFormat);QSettings::setPath(QSettings::IniFormat,QSettings::UserScope,dir.path());
        QFile response(dir.filePath("catalog.json"));QVERIFY(response.open(QIODevice::WriteOnly));response.write(QJsonDocument(catalog()).toJson());response.close();
        QFile program(script());QVERIFY(program.open(QIODevice::WriteOnly));program.write("#!/bin/sh\ncd \"$(dirname \"$0\")\"\nprintf '%s\\n' \"$*\" >> calls\ncase \"$1\" in @*) shift;; esac\ncase \"$1\" in\nworktrees) if [ \"$2\" = create ]; then shift 2; while [ $# -gt 1 ]; do case \"$1\" in --request-id) token=$2;; --branch) branch=$2;; --destination) destination=$2;; esac; shift 2; done; sleep 0.05; if [ -f fail-create ]; then echo 'Branch already exists' >&2; exit 1; fi; printf '{\"status\":\"created\",\"request_id\":\"%s\",\"branch\":\"%s\",\"path\":\"%s\",\"common_dir\":\"/repo/.git\",\"machine\":\"fixture\"}' \"$token\" \"$branch\" \"$destination\"; else sleep 0.05; cat catalog.json; fi;;\naccount) echo '{\"profiles\":[{\"id\":\"work\",\"provider\":\"codex\",\"label\":\"Work\"}]}' ;;\ndirs) printf '{\"path\":\"%s\",\"directories\":[]}' \"$2\";;\nesac\n");program.close();QVERIFY(program.setPermissions(QFileDevice::ReadOwner|QFileDevice::WriteOwner|QFileDevice::ExeOwner));
    }
    void init(){QSettings().clear();QFile::remove(dir.filePath("calls"));QFile::remove(dir.filePath("fail-create"));}
    void clientCachesCoalescesAndScopesSnapshots(){
        HgsClient client(script());QSignalSpy ready(&client,&HgsClient::worktreesReady);
        const auto first=client.requestWorktrees({},"/repo"),second=client.requestWorktrees({},"/repo");QVERIFY(first!=second);QTRY_COMPARE(ready.size(),2);
        QFile calls(dir.filePath("calls"));QVERIFY(calls.open(QIODevice::ReadOnly));QCOMPARE(calls.readAll().count("worktrees"),1);calls.close();
        QVERIFY(client.worktreeSnapshot("mac","/repo").isEmpty());QCOMPARE(client.worktreeSnapshot({},"/linked")["selected_root"].toString(),QString("/linked"));
        client.requestWorktrees({},"/linked");QTRY_COMPARE(ready.size(),3);QVERIFY(calls.open(QIODevice::ReadOnly));QCOMPARE(calls.readAll().count("worktrees"),1);calls.close();
        client.requestWorktrees("mac","/repo");QTRY_COMPARE(ready.size(),4);QVERIFY(!client.worktreeSnapshot("mac","/repo").isEmpty());
        client.requestWorktrees({},"/repo",true);QTRY_COMPARE(ready.size(),5);
    }
    void catalogGuardsUnavailableFoldersAndKeepsOfflineSnapshot(){
        HgsClient client(script());WorktreePanel panel(&client);auto state=fleet();panel.setContext({},"/repo",state);panel.resize(580,400);panel.show();
        auto *tree=panel.findChild<QTreeWidget *>("worktreeCatalog");QTRY_COMPARE(tree->topLevelItemCount(),4);
        QCOMPARE(tree->topLevelItem(0)->childCount(),1);QVERIFY(tree->topLevelItem(0)->child(0)->text(0).contains("codex/repo/one"));
        auto *use=panel.findChild<QPushButton *>("useWorktreeFolder");tree->setCurrentItem(tree->topLevelItem(2));QVERIFY(!use->isEnabled());tree->setCurrentItem(tree->topLevelItem(3));QVERIFY(!use->isEnabled());
        tree->setCurrentItem(tree->topLevelItem(1));QVERIFY(use->isEnabled());QString chosen;panel.launchRequested=[&](const QString &path){chosen=path;};use->click();QCOMPARE(chosen,QString("/linked"));
        auto local=state.local();local.ok=false;state.setLocal(local,QDateTime::currentMSecsSinceEpoch());panel.setContext({},"/repo",state);QCOMPARE(tree->topLevelItemCount(),4);QVERIFY(!use->isEnabled());QVERIFY(panel.findChild<QLabel *>("worktreeStatus")->text().contains("Offline"));
        QVERIFY(tree->topLevelItem(0)->child(0)->text(0).contains("last recorded"));
        const auto preview=qEnvironmentVariable("HGS_WORKTREE_PREVIEW");if(!preview.isEmpty()){QDir().mkpath(preview);QVERIFY(panel.grab().save(preview+"/offline.png"));}
    }
    void lateResponseCannotReplaceAnotherMachineOrFolder(){
        HgsClient client(script());WorktreePanel panel(&client);auto state=fleet();panel.show();panel.setContext({},"/repo",state);panel.setContext("mac","/repo",state);
        client.worktreesReady(1,{},"/repo",QJsonObject{{"state","not_repo"},{"path","/wrong"},{"worktrees",QJsonArray{}}});
        auto *tree=panel.findChild<QTreeWidget *>("worktreeCatalog");QTRY_COMPARE(tree->topLevelItemCount(),4);QCOMPARE(tree->topLevelItem(0)->childCount(),1);QVERIFY(tree->topLevelItem(0)->child(0)->text(0).contains("kimi/repo/two"));
        panel.setContext("mac","/plain",state);client.worktreesReady(2,"mac","/repo",catalog());QVERIFY(!panel.findChild<QLabel *>("worktreeStatus")->text().contains("/wrong"));
    }
    void pickerPreservesProjectMachineAccountAndCustomFolderAcrossPolls(){
        auto state=fleet();SessionOrganization org;const auto project=org.createGroup("Work");const auto id=org.addFolder(project,"mac","/repo");
        NewSessionDialog dialog(script(),state,"mac","codex",project);dialog.setGroups(org,project);QVERIFY(dialog.selectFolder(id));dialog.show();
        auto *account=dialog.findChild<QComboBox *>("launchAccount");QTRY_VERIFY(account->findData("work")>=0);account->setCurrentIndex(account->findData("work"));
        auto *choose=dialog.findChild<QPushButton *>("chooseLaunchWorktree");QTRY_VERIFY(choose->isVisible());
        QTimer::singleShot(0,&dialog,[&]{auto *picker=dialog.findChild<QDialog *>("chooseWorktreeDialog");QVERIFY(picker);QTimer::singleShot(3000,picker,&QDialog::reject);auto *tree=picker->findChild<QTreeWidget *>("worktreeCatalog");QTRY_COMPARE(tree->topLevelItemCount(),4);tree->setCurrentItem(tree->topLevelItem(1));auto *use=picker->findChild<QPushButton *>("useWorktreeFolder");QTRY_VERIFY(use->isEnabled());use->click();});choose->click();
        QCOMPARE(dialog.findChild<QLabel *>("launchFolderPath")->text(),QString("/linked"));dialog.setFleet(state);
        QCOMPARE(dialog.findChild<QComboBox *>("launchProject")->currentData().toString(),project);QCOMPARE(dialog.findChild<QComboBox *>("launchComputer")->currentData().toString(),QString("mac"));QCOMPARE(account->currentData().toString(),QString("work"));QCOMPARE(dialog.findChild<QLabel *>("launchFolderPath")->text(),QString("/linked"));
        QCOMPARE(org.group(project)->folders.size(),1);QSignalSpy launched(&dialog,&NewSessionDialog::launchRequested);dialog.findChild<QPushButton *>("primary")->click();QTRY_COMPARE(launched.size(),1);QCOMPARE(launched.first()[2].toString(),QString("/linked"));QCOMPARE(launched.first()[4].toString(),QString("work"));QCOMPARE(org.group(project)->folders.size(),1);
    }
    void accountPresetWaitsForCatalogAndNeverFallsBackSilently(){
        auto state=fleet();SessionOrganization org;const auto project=org.createGroup("Work");org.addFolder(project,"mac","/repo");
        NewSessionDialog dialog(script(),state,"mac","codex",project);dialog.setGroups(org,project);dialog.selectAccount("work");dialog.show();
        auto *account=dialog.findChild<QComboBox *>("launchAccount");auto *start=dialog.findChild<QPushButton *>("primary");
        QVERIFY(!start->isEnabled());QTRY_COMPARE(account->currentData().toString(),QString("work"));QTRY_VERIFY(start->isEnabled());
        dialog.setFleet(state);QCOMPARE(account->currentData().toString(),QString("work"));
        dialog.selectAccount("removed");QVERIFY(!start->isEnabled());QCOMPARE(account->currentIndex(),-1);
        QVERIFY(dialog.findChild<QLabel *>("launchError")->text().contains("unavailable"));
        dialog.setFleet(state);QCOMPARE(account->currentIndex(),-1);
        // This catalog contains only "work". A missing native profile must not
        // silently fall back to another account, just like any other missing ID.
        dialog.selectAccount("native-codex");QCOMPARE(account->currentIndex(),-1);QVERIFY(!start->isEnabled());
        dialog.selectAccount("work");QCOMPARE(account->currentData().toString(),QString("work"));QVERIFY(start->isEnabled());
    }
    void newWorktreeCreatesExplicitlyAndPreservesLaunchContext(){
        auto state=fleet();SessionOrganization org;const auto project=org.createGroup("Work");const auto id=org.addFolder(project,"mac","/repo");
        NewSessionDialog dialog(script(),state,"mac","codex",project);dialog.setGroups(org,project);QVERIFY(dialog.selectFolder(id));dialog.show();
        auto *account=dialog.findChild<QComboBox *>("launchAccount");QTRY_VERIFY(account->findData("work")>=0);account->setCurrentIndex(account->findData("work"));
        auto *create=dialog.findChild<QPushButton *>("newLaunchWorktree");QTRY_VERIFY(create->isVisible()&&create->isEnabled());
        QTimer::singleShot(0,&dialog,[&]{auto *form=dialog.findChild<QDialog *>("newWorktreeDialog");QVERIFY(form);form->reject();});create->click();
        QFile calls(dir.filePath("calls"));QVERIFY(calls.open(QIODevice::ReadOnly));QVERIFY(!calls.readAll().contains("worktrees create"));calls.close();
        QTimer::singleShot(0,&dialog,[&]{
            auto *form=dialog.findChild<QDialog *>("newWorktreeDialog");QVERIFY(form);QTimer::singleShot(3000,form,&QDialog::reject);
            form->findChild<QLineEdit *>("newWorktreeBranch")->setText("feature/new");
            form->findChild<QComboBox *>("newWorktreeBase")->setCurrentText("origin/main");
            form->findChild<QLineEdit *>("newWorktreePath")->setText("/new folder");
            auto *submit=form->findChild<QPushButton *>("createWorktree");submit->click();QVERIFY(!submit->isEnabled());form->reject();QVERIFY(form->isVisible());
        });create->click();
        QCOMPARE(dialog.findChild<QLabel *>("launchFolderPath")->text(),QString("/new folder"));dialog.setFleet(state);
        QCOMPARE(dialog.findChild<QComboBox *>("launchProject")->currentData().toString(),project);QCOMPARE(dialog.findChild<QComboBox *>("launchComputer")->currentData().toString(),QString("mac"));QCOMPARE(account->currentData().toString(),QString("work"));
        QCOMPARE(dialog.findChild<QLabel *>("launchFolderPath")->text(),QString("/new folder"));QCOMPARE(org.group(project)->folders.size(),1);
        QSignalSpy launched(&dialog,&NewSessionDialog::launchRequested);dialog.findChild<QPushButton *>("primary")->click();QTRY_COMPARE(launched.size(),1);QCOMPARE(launched.first()[2].toString(),QString("/new folder"));
        QVERIFY(calls.open(QIODevice::ReadOnly));const auto commands=calls.readAll();QCOMPARE(commands.count("worktrees create"),1);QVERIFY(commands.contains("@mac worktrees create"));QVERIFY(commands.contains("--base origin/main"));
    }
    void failedCreationRetainsFormAndOriginalFolder(){
        QFile fail(dir.filePath("fail-create"));QVERIFY(fail.open(QIODevice::WriteOnly));fail.close();
        auto state=fleet();SessionOrganization org;const auto project=org.createGroup("Work");org.addFolder(project,"arch","/repo");
        NewSessionDialog dialog(script(),state,{},"codex",project);dialog.setGroups(org,project);dialog.show();
        auto *create=dialog.findChild<QPushButton *>("newLaunchWorktree");QTRY_VERIFY(create->isVisible()&&create->isEnabled());
        QTimer::singleShot(0,&dialog,[&]{
            auto *form=dialog.findChild<QDialog *>("newWorktreeDialog");QVERIFY(form);QTimer::singleShot(3000,form,&QDialog::reject);
            form->findChild<QLineEdit *>("newWorktreeBranch")->setText("taken");form->findChild<QPushButton *>("createWorktree")->click();
            auto *error=form->findChild<QLabel *>("newWorktreeError");QTRY_VERIFY(error->text().contains("already exists"));
            QCOMPARE(form->findChild<QLineEdit *>("newWorktreeBranch")->text(),QString("taken"));QVERIFY(form->findChild<QPushButton *>("createWorktree")->isEnabled());form->reject();
        });create->click();
        QCOMPARE(dialog.findChild<QLabel *>("launchFolderPath")->text(),QString("/repo"));QCOMPARE(org.group(project)->folders.size(),1);
    }
};
QTEST_MAIN(TestWorktreePanel)
#include "test_worktreepanel.moc"
