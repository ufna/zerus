#include <QtTest>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSettings>
#include <QTemporaryDir>
#include "SwarmController.h"
#include "SwarmDialog.h"
#include "FleetState.h"
#include <QApplication>
#include <QComboBox>
#include <QDialog>
#include <QPushButton>
#include <QTableWidget>
#include <QTabWidget>
#include <QStackedWidget>
#include <QLineEdit>
#include <QLabel>
#include <QScrollBar>

class TestSwarmController : public QObject {
    Q_OBJECT
    QTemporaryDir m_directory;
    QString m_program, m_root;
    const QString m_key="name-field";
    QJsonObject organization(const QString &name) const {
        return {{"version",2},{"default_project","p"},{"projects",QJsonArray{QJsonObject{{"id","p"},{"name",name},{"color","#123456"},{"vivid",false},{"folders",QJsonArray{}},{"sessions",QJsonArray{}}}}}};
    }
    void write(const QString &name,const QByteArray &bytes) {QFile file(m_root+'/'+name);QVERIFY(file.open(QIODevice::WriteOnly));file.write(bytes);}
    void response(const QString &name) {
        write("response",QJsonDocument(QJsonObject{{"schema",1},{"organization",organization(name)},{"versions",QJsonObject{{m_key,QJsonArray{"observed"}}}},{"peers",QJsonObject{}},{"conflicts",QJsonArray{}}}).toJson());
    }
    QJsonArray requests() const {
        QFile file(m_root+"/requests");QJsonArray result;if(!file.open(QIODevice::ReadOnly))return result;
        for(const auto &line:file.readAll().split('\n'))if(!line.isEmpty())result.append(QJsonDocument::fromJson(line).object());return result;
    }
private slots:
    void initTestCase() {
        QVERIFY(m_directory.isValid());QCoreApplication::setOrganizationName("hgs-swarm-test");QCoreApplication::setApplicationName("controller");
        QSettings::setDefaultFormat(QSettings::IniFormat);QSettings::setPath(QSettings::IniFormat,QSettings::UserScope,m_directory.path());
    }
    void init() {
        QSettings().clear();m_root=m_directory.path()+"/case-"+QString::number(QDateTime::currentMSecsSinceEpoch());QDir().mkpath(m_root);m_program=m_root+"/hgs";
        const QByteArray script=R"PY(#!/usr/bin/env python3
import json,pathlib,sys,time
root=pathlib.Path(__file__).parent
action=sys.argv[2]
payload=json.load(sys.stdin) if action in ['apply','initialize'] else {}
with (root/'requests').open('a') as file:file.write(json.dumps(dict(action=action,input=payload))+'\n')
count=len((root/'requests').read_text().splitlines())
while (root/'hold').exists():time.sleep(.01)
if (root/'fail').exists():print('temporary fixture failure',file=sys.stderr);sys.exit(1)
result=json.loads((root/('preview' if action=='preview' else 'response')).read_text())
if action=='apply':
    result['organization']=payload['desired']
    result['written']={'name-field':['own-'+str(count)]}
    result['versions']={'name-field':['own-'+str(count),'unseen-remote']}
print(json.dumps(result))
)PY";
        write("hgs",script);QFile::setPermissions(m_program,QFile::ReadOwner|QFile::WriteOwner|QFile::ExeOwner);response("Original");
    }
    void localPresentationSurvivesRemoteMembership() {
        auto local=organization("Old");auto project=local["projects"].toArray()[0].toObject();project["collapsed"]=true;project["sessions"]=QJsonArray{"b","a","removed"};local["projects"]=QJsonArray{project};local["runs"]=QJsonObject{{"run","a"}};
        auto remote=organization("Renamed");project=remote["projects"].toArray()[0].toObject();project["sessions"]=QJsonArray{"a","new","b"};remote["projects"]=QJsonArray{project};
        const auto result=SwarmController::presentation(remote,local);const auto p=result["projects"].toArray()[0].toObject();
        QCOMPARE(p["name"].toString(),"Renamed");QVERIFY(p["collapsed"].toBool());QCOMPARE(p["sessions"].toArray(),QJsonArray({"b","a","new"}));QCOMPARE(result["runs"],local["runs"]);
    }
    void editDuringRefreshUsesVersionsTheUserSaw() {
        HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Original"));QTRY_VERIFY(controller.settled());
        write("hold",{});response("Remote change");controller.refresh();QTRY_COMPARE(requests().size(),2);
        controller.edit(organization("Local change"));QFile::remove(m_root+"/hold");
        QTRY_COMPARE(requests().size(),3);QTRY_VERIFY(controller.settled());
        const auto patch=requests()[2].toObject()["input"].toObject();QCOMPARE(patch["versions"].toObject()[m_key].toArray(),QJsonArray{"observed"});
        QCOMPARE(patch["base"].toObject()["projects"].toArray()[0].toObject()["name"].toString(),"Original");
        QCOMPARE(controller.snapshot()["organization"].toObject()["projects"].toArray()[0].toObject()["name"].toString(),"Local change");
    }
    void editDuringWriteAdvancesOnlyOwnAcknowledgedVersions() {
        HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Original"));QTRY_VERIFY(controller.settled());
        write("hold",{});controller.edit(organization("First"));QTRY_COMPARE(requests().size(),2);
        controller.edit(organization("Second"));QFile::remove(m_root+"/hold");QTRY_COMPARE(requests().size(),3);QTRY_VERIFY(controller.settled());
        const auto patch=requests()[2].toObject()["input"].toObject();QCOMPARE(patch["versions"].toObject()[m_key].toArray(),QJsonArray{"own-2"});
        QCOMPARE(patch["base"].toObject()["projects"].toArray()[0].toObject()["name"].toString(),"First");
    }
    void restartRetriesUnsavedDraftInsteadOfReplacingIt() {
        {
            HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Original"));QTRY_VERIFY(controller.settled());
            write("fail",{});controller.edit(organization("Unsaved"));QTRY_COMPARE(requests().size(),2);QTRY_VERIFY(!controller.busy());
        }
        QFile::remove(m_root+"/fail");response("A newer remote value");
        HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Unsaved"));QTRY_COMPARE(requests().size(),4);QTRY_VERIFY(controller.settled());
        const auto patch=requests()[3].toObject()["input"].toObject();QCOMPARE(patch["versions"].toObject()[m_key].toArray(),QJsonArray{"observed"});
        QCOMPARE(patch["desired"].toObject()["projects"].toArray()[0].toObject()["name"].toString(),"Unsaved");
    }
    void unfinishedTextFieldDefersRemoteRefresh() {
        HgsClient client(m_program);SwarmController controller(&client);QSignalSpy rendered(&controller,&SwarmController::organizationReady);
        controller.start(organization("Original"));QTRY_VERIFY(controller.settled());QCOMPARE(rendered.count(),1);
        write("hold",{});response("Remote name");controller.refresh();QTRY_COMPARE(requests().size(),2);
        controller.setDraftEditing(true);QFile::remove(m_root+"/hold");QTRY_VERIFY(!controller.busy());QCOMPARE(rendered.count(),1);
        controller.edit(organization("Typed name"));controller.setDraftEditing(false);QTRY_COMPARE(requests().size(),3);QTRY_VERIFY(controller.settled());
        const auto patch=requests()[2].toObject()["input"].toObject();QCOMPARE(patch["versions"].toObject()[m_key].toArray(),QJsonArray{"observed"});
    }
    void largeFleetKeepsScrollAndSelection() {
        HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Original"));QTRY_VERIFY(controller.settled());
        auto snapshot=controller.snapshot();QJsonArray machines;
        for(int i=0;i<30;++i)machines.append(QJsonObject{{"id",QString::number(i)},{"name",QString("Computer %1").arg(i,2,10,QChar('0'))},{"connection",QString("peer-%1").arg(i)}});
        snapshot["machines"]=machines;controller.acceptExternal(snapshot);FleetState fleet;bool checked=false;
        QTimer::singleShot(0,this,[&]{
            auto *dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());QVERIFY(dialog);
            QTimer::singleShot(50,dialog,[&,dialog]{
                auto *table=dialog->findChild<QTableWidget*>("swarmMachines");auto *filter=dialog->findChild<QLineEdit*>("swarmMachineFilter");
                QCOMPARE(table->rowCount(),30);QVERIFY(table->height()>300);QVERIFY(table->verticalScrollBar()->maximum()>0);
                table->setCurrentCell(25,0);table->scrollToItem(table->item(25,0));
                const auto selected=table->item(table->currentRow(),0)->data(Qt::UserRole);const int scroll=table->verticalScrollBar()->value();QVERIFY(scroll>0);
                auto updated=snapshot;updated["peers"]=QJsonObject{{"peer-25",QJsonObject{{"last_sync",12345}}}};
                controller.acceptExternal(updated);QCOMPARE(table->item(table->currentRow(),0)->data(Qt::UserRole),selected);QCOMPARE(table->verticalScrollBar()->value(),scroll);
                const auto preview=qEnvironmentVariable("HGS_SWARM_PREVIEW");if(!preview.isEmpty())QVERIFY(dialog->grab().save(preview+"-computers.png"));
                filter->setText("PEER-25");int visible=0;for(int row=0;row<30;++row)if(!table->isRowHidden(row))++visible;
                QCOMPARE(visible,1);QCOMPARE(dialog->findChild<QLabel*>("swarmMachineCount")->text(),"1 of 30 computers");
                controller.acceptExternal(updated);QCOMPARE(filter->text(),"PEER-25");QVERIFY(!table->isRowHidden(table->currentRow()));
                filter->clear();for(int row=0;row<30;++row)QVERIFY(!table->isRowHidden(row));checked=true;dialog->reject();
            });
        });
        QTimer timeout;timeout.setSingleShot(true);connect(&timeout,&QTimer::timeout,this,[]{if(auto *dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget()))dialog->reject();});timeout.start(10000);
        showSwarmDialog(&client,&controller,fleet,nullptr,[]{});QVERIFY(checked);
    }
    void previewsDoNotMoveSurroundingControls() {
        HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Original"));QTRY_VERIFY(controller.settled());
        FleetState fleet;BoxState remote;remote.host="mac";remote.ok=true;fleet.setPeer(remote,0);bool checked=false;
        QTimer::singleShot(0,this,[&]{
            auto *dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());QVERIFY(dialog);
            auto *tabs=dialog->findChild<QTabWidget*>("swarmTabs");tabs->setCurrentIndex(1);
            auto *preview=dialog->findChild<QPushButton*>("swarmPreview");auto *join=dialog->findChild<QPushButton*>("swarmJoin");auto *status=dialog->findChild<QLabel*>("swarmStatus");
            QTest::qWait(30);const auto size=dialog->size();const auto tabsRect=tabs->geometry();const auto statusRect=status->geometry();const auto joinBottom=join->mapTo(dialog,join->rect().bottomRight());const auto previewRect=preview->geometry();
            for(bool same:{false,true,false}){
                write("preview",QJsonDocument(QJsonObject{{"node_id","node-b"},{"swarm_id","swarm-b"},{"same_swarm",same},{"initialized",true},{"projects",QJsonArray{QJsonObject{{"id","remote-id"},{"name","Remote project"}}}}}).toJson());
                preview->click();QTRY_VERIFY(join->isEnabled());QTest::qWait(20);
                QCOMPARE(dialog->size(),size);QCOMPARE(tabs->geometry(),tabsRect);QCOMPARE(status->geometry(),statusRect);QCOMPARE(join->mapTo(dialog,join->rect().bottomRight()),joinBottom);QCOMPARE(preview->geometry(),previewRect);
            }
            tabs->setCurrentIndex(2);QTest::qWait(20);QCOMPARE(dialog->size(),size);QCOMPARE(status->geometry(),statusRect);
            QCOMPARE(dialog->findChild<QStackedWidget*>("swarmConflictReview")->currentIndex(),0);
            checked=true;dialog->reject();
        });
        QTimer timeout;timeout.setSingleShot(true);connect(&timeout,&QTimer::timeout,this,[]{if(auto *dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget()))dialog->reject();});timeout.start(15000);
        showSwarmDialog(&client,&controller,fleet,nullptr,[]{});QVERIFY(checked);
    }
    void joinPreviewRequiresExplicitProjectMapping() {
        HgsClient client(m_program);SwarmController controller(&client);controller.start(organization("Original"));QTRY_VERIFY(controller.settled());
        write("preview",QJsonDocument(QJsonObject{{"node_id","node-b"},{"swarm_id","swarm-b"},{"same_swarm",false},{"initialized",true},{"projects",QJsonArray{QJsonObject{{"id","different-id"},{"name","Original"}}}}}).toJson());
        FleetState fleet;BoxState local;local.host="arch";local.ok=true;fleet.setLocal(local,0);BoxState remote;remote.host="mac";remote.ok=true;fleet.setPeer(remote,0);
        bool checked=false;
        QTimer::singleShot(0,this,[&]{
            auto *dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());QVERIFY(dialog);
            dialog->findChild<QTabWidget*>("swarmTabs")->setCurrentIndex(1);
            auto *button=dialog->findChild<QPushButton*>("swarmPreview");QVERIFY(button);button->click();
            QTimer::singleShot(300,dialog,[&,dialog]{
                auto *table=dialog->findChild<QTableWidget*>("swarmProjectMapping");QVERIFY(table);QCOMPARE(table->rowCount(),1);
                auto *choice=qobject_cast<QComboBox*>(table->cellWidget(0,1));QVERIFY(choice);QCOMPARE(choice->count(),2);QVERIFY(choice->currentData().toString().isEmpty());
                auto *join=dialog->findChild<QPushButton*>("swarmJoin");QVERIFY(join);QVERIFY(join->isEnabled());
                const auto preview=qEnvironmentVariable("HGS_SWARM_PREVIEW");if(!preview.isEmpty())QVERIFY(dialog->grab().save(preview));
                checked=true;dialog->reject();
            });
        });
        QTimer timeout;timeout.setSingleShot(true);connect(&timeout,&QTimer::timeout,this,[]{if(auto *dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget()))dialog->reject();});timeout.start(10000);
        showSwarmDialog(&client,&controller,fleet,nullptr,[]{});QVERIFY(checked);
    }
};
QTEST_MAIN(TestSwarmController)
#include "test_swarmcontroller.moc"
