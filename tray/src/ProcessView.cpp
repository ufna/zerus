#include "ProcessView.h"
#include <algorithm>
#include "HgsClient.h"
#include "BusyIndicator.h"
#include "ProcessSettings.h"
#include <QApplication>
#include <QClipboard>
#include <QDateTime>
#include <QFontDatabase>
#include <QHeaderView>
#include <QJsonArray>
#include <QItemSelectionModel>
#include <QLabel>
#include <QMenu>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QSplitter>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace {
bool active(const QJsonObject &j) {return QStringList{"starting","approval","running","stopping","unknown"}.contains(j.value("status").toString());}
bool stoppable(const QJsonObject &j) {return active(j) && j.value("status")!="unknown" && j.value("status")!="stopping" && j.value("capabilities").toObject().value("stop").toBool();}
QJsonObject outputRevision(const QJsonObject &j) {return {{"status",j.value("status")},{"output_at",j.value("output_at")},{"ended_at",j.value("ended_at")}};}
QString status(const QJsonObject &j) {
    const auto s=j.value("status").toString();
    if(s=="unknown")return QObject::tr("Unconfirmed");
    if(s=="approval")return QObject::tr("Awaiting approval");
    if(s=="timed_out")return QObject::tr("Timed out");
    auto result=s; if(!result.isEmpty())result[0]=result[0].toUpper();return result;
}
}
ProcessView::ProcessView(HgsClient *client,QWidget *parent):QWidget(parent),m_client(client)
{
    setObjectName("processView");
    auto *layout=new QVBoxLayout(this);layout->setContentsMargins(12,12,12,12);
    auto *bar=new QHBoxLayout;m_summary=new QLabel; m_summary->setTextFormat(Qt::PlainText);bar->addWidget(m_summary,1);
    m_busy=new BusyIndicator;m_busy->setObjectName("processBusy");bar->addWidget(m_busy);
    m_stop=new QPushButton(tr("Stop selected"));m_stop->setObjectName("stopProcess");bar->addWidget(m_stop);
    layout->addLayout(bar);
    m_source=new QLabel;m_source->setTextFormat(Qt::PlainText);m_source->setWordWrap(true);m_source->setObjectName("hint");m_source->hide();layout->addWidget(m_source);
    m_stopNotice=new QLabel;m_stopNotice->setObjectName("processStopNotice");m_stopNotice->setTextFormat(Qt::PlainText);m_stopNotice->setWordWrap(true);m_stopNotice->hide();layout->addWidget(m_stopNotice);
    auto *split=new QSplitter(Qt::Vertical);layout->addWidget(split,1);
    m_list=new QTreeWidget; m_list->setObjectName("processList");m_list->setHeaderLabels({tr("Command"),tr("Agent"),tr("State"),tr("Duration")});
    m_list->setUniformRowHeights(true);m_list->setRootIsDecorated(true);m_list->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_list->header()->setStretchLastSection(false);m_list->header()->setSectionResizeMode(0,QHeaderView::Stretch);
    for(int i=1;i<4;++i)m_list->header()->setSectionResizeMode(i,QHeaderView::ResizeToContents);
    split->addWidget(m_list);
    m_active=new QTreeWidgetItem(m_list,{tr("Recorded commands / unconfirmed")});m_active->setFlags(Qt::ItemIsEnabled);m_active->setExpanded(true);
    m_finished=new QTreeWidgetItem(m_list,{tr("Finished")});m_finished->setFlags(Qt::ItemIsEnabled);m_finished->setExpanded(false);
    m_liveProcesses=new QTreeWidgetItem;m_liveProcesses->setText(0,tr("Live processes"));m_liveProcesses->setFlags(Qt::ItemIsEnabled);m_list->insertTopLevelItem(0,m_liveProcesses);m_liveProcesses->setExpanded(true);m_liveProcesses->setHidden(true);
    auto *bottom=new QWidget;auto *outputLayout=new QVBoxLayout(bottom);outputLayout->setContentsMargins(0,6,0,0);
    m_details=new QLabel;m_details->setObjectName("processDetails");m_details->setTextFormat(Qt::PlainText);m_details->setWordWrap(true);m_details->setTextInteractionFlags(Qt::TextSelectableByMouse);outputLayout->addWidget(m_details);
    auto *actions=new QHBoxLayout;m_notice=new QLabel;m_notice->setTextFormat(Qt::PlainText);m_notice->setWordWrap(true);actions->addWidget(m_notice,1);
    m_copy=new QPushButton(tr("Copy command"));actions->addWidget(m_copy);
    outputLayout->addLayout(actions);
    m_output=new QPlainTextEdit;m_output->setObjectName("processOutput");m_output->setReadOnly(true);m_output->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));m_output->setPlaceholderText(tr("Select a command to view its output."));outputLayout->addWidget(m_output,1);split->addWidget(bottom);split->setSizes({260,320});
    connect(m_list,&QTreeWidget::itemSelectionChanged,this,&ProcessView::select);
    connect(m_list,&QTreeWidget::currentItemChanged,this,[this]{select();});
    connect(m_copy,&QPushButton::clicked,this,[this]{QApplication::clipboard()->setText(m_jobs.value(m_selected).value("command").toString());});
    connect(m_stop,&QPushButton::clicked,this,&ProcessView::stopSelected);
    m_list->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_list,&QTreeWidget::customContextMenuRequested,this,[this](const QPoint &point){
        auto *row=m_list->itemAt(point);
        if(!row || !m_jobs.contains(row->data(0,Qt::UserRole).toString()))return;
        if(!row->isSelected())m_list->setCurrentItem(row,0,QItemSelectionModel::ClearAndSelect);
        updateActions();const auto key=m_key;const auto ids=selectedIds();
        QMenu menu(this);menu.setObjectName("processContextMenu");
        auto *terminate=menu.addAction(ids.size()==1?tr("Terminate"):tr("Terminate selected (%1)").arg(ids.size()));
        terminate->setObjectName("terminateProcesses");terminate->setEnabled(m_stop->isEnabled());terminate->setToolTip(m_stop->toolTip());
        menu.setToolTipsVisible(true);
        if(menu.exec(m_list->viewport()->mapToGlobal(point))==terminate && key==m_key && ids==selectedIds())stopSelected();
    });
    m_busyDelay.setSingleShot(true);
    connect(&m_busyDelay,&QTimer::timeout,this,[this]{if(isVisible()&&(m_request||m_stopRequest))m_busy->setRunning(true);});
    connect(m_client,&HgsClient::processFinished,this,[this](quint64 requestId,const QJsonObject &data,const QString &error){
        if(requestId==m_stopRequest){
            m_stopRequest=0;
            const bool receipt=data.value("id").toString()==m_stopId && data.value("run_id").toString()==m_run && data.value("conversation_id").toString()==m_conversation;
            if(!error.isEmpty() || !receipt || (data.value("status")!="requested" && data.value("status")!="already-finished")){
                m_stopPending.remove(m_stopId);m_stopErrors<<tr("%1: %2").arg(m_stopId.left(12),error.isEmpty()?tr("Stop was not confirmed"):error);
            }else if(data.value("status")=="already-finished"){
                m_stopPending.remove(m_stopId);++m_stopSkipped;
            }else ++m_stopSent;
            stopNext();emit refreshRequested();return;
        }
        if(requestId!=m_request)return;
        m_request=0;updateBusy();updateActions();
        if(m_requestProcess!=m_selected)return;
        if(!error.isEmpty()){m_notice->setText(error);return;}
        if(data.value("id").toString()!=m_selected || data.value("run_id").toString()!=m_run || data.value("conversation_id").toString()!=m_conversation){m_notice->setText(tr("Session changed. Refreshing processes…"));emit refreshRequested();return;}
        const auto text=data.value("output").toString();
        if(m_output->toPlainText()!=text){auto *scroll=m_output->verticalScrollBar();const bool end=scroll->value()>=scroll->maximum()-2;const int previous=scroll->value();m_output->setPlainText(text);scroll->setValue(end?scroll->maximum():previous);}
        m_notice->setText(data.value("truncated").toBool()?tr("Showing the retained output tail."):text.isEmpty()?tr("No output recorded yet."):QString());
    });
    applyPreferences();connect(&m_timer,&QTimer::timeout,this,[this]{refreshDurations();requestOutput();});select();

}
void ProcessView::applyPreferences(){
    m_timer.setInterval(ProcessSettings::activeMs());m_busyDelay.setInterval(ProcessSettings::loadingMs());
    if(!ProcessSettings::enabled()){m_timer.stop();m_busyDelay.stop();m_busy->setRunning(false);m_request=0;m_stopQueue.clear();}
    else if(isVisible())m_timer.start();
}
void ProcessView::setTheme(bool dark){m_dark=dark;m_busy->setTheme(dark);refreshDurations();}
void ProcessView::saveView(){if(!m_key.isEmpty())m_views[m_key]={m_selected,m_output->toPlainText(),selectedIds(),m_output->verticalScrollBar()->value(),m_list->verticalScrollBar()->value()};while(m_views.size()>30)m_views.erase(m_views.begin());}
void ProcessView::setSession(const QString &host,const QString &name,const QString &archive,const QJsonObject &inspection,bool online,bool live)
{
    const auto run=inspection.value("run_id").toString(),conversation=inspection.value("conversation_id").toString();
    const auto generation=inspection.value("host_generation").toString();
    const QString key=QStringList{host,name,archive,run,conversation,generation}.join('\n');const bool changed=key!=m_key;
    if(changed){saveView();m_request=0;m_stopRequest=0;m_stopQueue.clear();m_stopPending.clear();m_stopNotice->clear();m_stopNotice->hide();m_key=key;m_selected.clear();m_output->clear();m_outputAge.invalidate();updateBusy();}
    m_host=host;m_name=name;m_archive=archive;m_run=run;m_conversation=conversation;m_generation=generation;m_online=online;m_live=live;
    const auto processes=inspection.value("processes").toObject();
    const auto prior=m_jobs;const auto selection=changed?m_views.value(key).selection:selectedIds();m_jobs.clear();
    {QSignalBlocker blocker(m_list);
        for(const auto &value:processes.value("items").toArray()){
            auto j=value.toObject();const auto id=j.value("id").toString();if(id.isEmpty())continue;
            if(!live && active(j)){j["last_status"]=j.value("status");j["status"]="unknown";}
            m_jobs[id]=j;auto *parent=(j.value("source")=="process_tree" || (live && j.value("background").toBool() && j.value("observed").toBool() && j.value("status")=="running"))?m_liveProcesses:active(j)?m_active:m_finished;auto *row=m_rows.value(id);
            if(!row){row=new QTreeWidgetItem(parent);m_rows[id]=row;row->setData(0,Qt::UserRole,id);}
            else if(row->parent()!=parent){row->parent()->takeChild(row->parent()->indexOfChild(row));parent->addChild(row);}
            auto command=j.value("command").toString();row->setText(0,command.simplified());row->setToolTip(0,command);row->setText(1,j.value("owner").toString());row->setText(2,status(j));row->setToolTip(2,j.value("status")=="unknown"?tr("Last recorded state: %1. This process has no current live confirmation.").arg(j.value("last_status").toString()):j.value("detail").toString());
        }
        for(const auto &id:m_rows.keys())if(!m_jobs.contains(id)){delete m_rows.take(id);}
        m_active->setHidden(!m_active->childCount());m_finished->setHidden(!m_finished->childCount());m_liveProcesses->setHidden(!m_liveProcesses->childCount());
        const auto desired=changed?m_views.value(key).selected:m_selected;
        for(auto i=m_rows.begin();i!=m_rows.end();++i)i.value()->setSelected(selection.contains(i.key()));
        if(m_rows.contains(desired)){if(m_list->currentItem()!=m_rows[desired])m_list->setCurrentItem(m_rows[desired],0,QItemSelectionModel::NoUpdate);}
        else if(!m_rows.isEmpty() && (changed || prior.isEmpty()))m_list->setCurrentItem(m_liveProcesses->childCount()?m_liveProcesses->child(0):m_active->childCount()?m_active->child(0):m_finished->child(0),0,QItemSelectionModel::ClearAndSelect);

    }
    m_liveProcesses->setText(0,live?tr("Live shells / processes"):tr("Last observed processes"));
    int unconfirmed=0,running=0,liveProcesses=0;for(const auto &j:m_jobs){if(j.value("source")=="process_tree" && live)++liveProcesses;else if(j.value("status")=="unknown")++unconfirmed;else if(active(j))++running;}
    m_summary->setText(m_jobs.isEmpty()?tr("No shell commands recorded yet"):(liveProcesses?tr("%1 live    ").arg(liveProcesses):QString())+tr("%1 active    %2 unconfirmed    %3 recorded").arg(running).arg(unconfirmed).arg(m_jobs.size()-liveProcesses));
    QStringList notes;for(const auto &note:processes.value("notes").toArray())notes<<note.toString();
    m_source->setText(notes.join('\n'));m_source->setVisible(!notes.isEmpty());
    m_summary->setToolTip(notes.join('\n')+(processes.value("complete").toBool()?QString():tr("\nOnly observed commands are shown; older history may be unavailable.")));
    for(const auto &id:m_stopPending.values())if(!m_jobs.contains(id) || !active(m_jobs[id]))m_stopPending.remove(id);
    select();refreshDurations();
    if(changed){const auto saved=m_views.value(key);if(saved.selected==m_selected){m_output->setPlainText(saved.output);m_output->verticalScrollBar()->setValue(saved.scroll);}m_list->verticalScrollBar()->setValue(saved.listScroll);}
    if(!changed && outputRevision(prior.value(m_selected))!=outputRevision(m_jobs.value(m_selected))){m_outputDirty=true;requestOutput();}
}
void ProcessView::select(){
    auto *row=m_list->currentItem();const auto id=row?row->data(0,Qt::UserRole).toString():QString();const bool changed=id!=m_selected;
    if(changed){m_selected=id;m_request=0;m_outputAge.invalidate();m_outputDirty=true;updateBusy();m_output->clear();m_notice->clear();}
    const auto job=m_jobs.value(id);m_copy->setEnabled(!job.isEmpty());
    updateActions();
    const auto cwd=job.value("cwd").toString();const auto command=job.value("command").toString();
    QStringList details{command.left(300)+(command.size()>300?QStringLiteral("…"):QString()),QStringLiteral("%1  %2  (%3)").arg(m_host.isEmpty()?tr("This machine"):m_host,cwd,status(job))};
    const auto description=job.value("description").toString();if(!description.isEmpty())details<<description.left(200);
    const auto progress=job.value("progress").toString();if(!progress.isEmpty())details<<progress.left(200);
    if(!job.value("exit_code").isNull() && job.contains("exit_code"))details<<tr("Exit code: %1").arg(job.value("exit_code").toInt());
    m_details->setText(job.isEmpty()?QString():details.join('\n'));m_details->setMaximumHeight(125);
    if(changed)requestOutput(true);
}
QStringList ProcessView::selectedIds() const {
    QStringList ids;for(auto *row:m_list->selectedItems()){const auto id=row->data(0,Qt::UserRole).toString();if(m_jobs.contains(id))ids<<id;}return ids;
}
void ProcessView::updateActions(){
    const auto ids=selectedIds();
    m_stop->setText(ids.isEmpty()?tr("Stop selected"):tr("Stop selected (%1)").arg(ids.size()));
    QString reason;
    if(!ProcessSettings::enabled())reason=tr("Enable Processes in Settings to use process control.");
    else if(!m_online || !m_live)reason=tr("Process control requires a live session on an online machine.");
    else if(m_stopRequest || !m_stopQueue.isEmpty())reason=tr("Sending stop requests…");
    else if(ids.isEmpty())reason=tr("Select processes with Ctrl/Cmd or Shift.");
    else for(const auto &id:ids){
        const auto job=m_jobs.value(id);
        if(m_stopPending.contains(id) || job.value("status")=="stopping"){reason=tr("A stop request is already pending for a selected process.");break;}
        if(!active(job)){reason=tr("Select active processes to stop.");break;}
        if(!stoppable(job)){reason=tr("Targeted stop is unavailable for this recorded command. Select its running process in Live shells / processes, or a supported native job.");break;}
    }
    const bool osProcess=std::any_of(ids.begin(),ids.end(),[this](const QString &id){return m_jobs.value(id).value("source")=="process_tree";});
    m_stop->setEnabled(reason.isEmpty());m_stop->setToolTip(reason.isEmpty()?(osProcess?tr("Stop selected processes and their children: TERM, then KILL after 2 seconds if needed."):tr("Request native cancellation of exactly the selected processes.")):reason);
}
void ProcessView::updateBusy(){
    if((m_request || m_stopRequest) && isVisible()){
        if(!m_busy->isVisible() && !m_busyDelay.isActive())m_busyDelay.start();
    }else {m_busyDelay.stop();m_busy->setRunning(false);}
}
void ProcessView::requestOutput(bool force){
    const auto j=m_jobs.value(m_selected);
    if(!ProcessSettings::enabled() || !isVisible() || !m_online || m_request || j.isEmpty() || m_run.isEmpty() || m_conversation.isEmpty())return;
    if(j.value("capabilities").toObject().value("output")==false){m_notice->setText(tr("Live OS process. Output remains in the recorded command or Terminal."));return;}
    const int interval=j.value("status")=="unknown"?qMax(ProcessSettings::unknownMs(),ProcessSettings::activeMs()):ProcessSettings::activeMs();
    if(!force && ((!m_outputDirty && !active(j)) || (m_outputAge.isValid()&&m_outputAge.elapsed()<interval)))return;
    m_outputDirty=false;m_outputAge.start();m_requestProcess=m_selected;
    m_request=m_client->requestProcess(m_host,m_name,m_selected,m_run,m_conversation,m_archive,m_generation);
    updateBusy();
}
void ProcessView::stopSelected(){
    updateActions();if(!m_stop->isEnabled())return;
    m_stopQueue=selectedIds();m_stopKey=m_key;m_stopSent=0;m_stopSkipped=0;m_stopErrors.clear();
    m_stopNotice->setText(tr("Sending stop requests…"));m_stopNotice->show();stopNext();
}
void ProcessView::stopNext(){
    if(m_stopRequest)return;
    if(m_stopKey!=m_key)return;
    while(!m_stopQueue.isEmpty()){
        const auto id=m_stopQueue.takeFirst();const auto job=m_jobs.value(id);
        if(!ProcessSettings::enabled() || !m_online || !m_live){m_stopErrors<<tr("Machine or session became unavailable; remaining requests were cancelled.");m_stopQueue.clear();break;}
        if(job.isEmpty() || !active(job)){++m_stopSkipped;continue;}
        if(!stoppable(job)){m_stopErrors<<tr("%1: process control is no longer available").arg(id.left(12));continue;}
        m_stopId=id;m_stopPending.insert(id);
        m_stopRequest=m_client->requestProcess(m_host,m_name,id,m_run,m_conversation,m_archive,m_generation,true);
        updateActions();updateBusy();return;
    }
    QString message=tr("Stop requested for %1 process(es).").arg(m_stopSent);
    if(m_stopSent)message+=tr(" Waiting for completion.");
    if(m_stopSkipped)message+=tr(" %1 already finished or left the list.").arg(m_stopSkipped);
    if(!m_stopErrors.isEmpty())message+=tr(" %1 not confirmed. No automatic retries.").arg(m_stopErrors.size())+"\n"+m_stopErrors.join('\n');
    m_stopNotice->setText(message);m_stopNotice->show();updateActions();updateBusy();
}
void ProcessView::selectProcess(const QString &id){if(m_rows.contains(id)){m_list->setCurrentItem(m_rows[id],0,QItemSelectionModel::ClearAndSelect);m_list->scrollToItem(m_rows[id]);}else m_notice->setText(tr("This command is outside the retained process history."));}
void ProcessView::showEvent(QShowEvent *event){QWidget::showEvent(event);if(ProcessSettings::enabled())m_timer.start();requestOutput(true);updateBusy();}
void ProcessView::hideEvent(QHideEvent *event){m_timer.stop();m_busyDelay.stop();m_busy->setRunning(false);QWidget::hideEvent(event);}
void ProcessView::refreshDurations(){
    const auto now=QDateTime::currentSecsSinceEpoch();
    for(auto i=m_rows.begin();i!=m_rows.end();++i){const auto j=m_jobs.value(i.key());const auto started=j.value("started_at").toDouble();
        const auto end=j.value("ended_at").toDouble(j.value("status")=="unknown"?j.value("updated_at").toDouble():now);
        const auto seconds=qMax<qint64>(0,end-started);i.value()->setText(3,started>0?QStringLiteral("%1:%2%3").arg(seconds/60).arg(seconds%60,2,10,QLatin1Char('0')).arg(j.value("status")=="unknown"?QStringLiteral(" (recorded)"):QString()):QStringLiteral("—"));
        const auto s=j.value("status").toString();i.value()->setForeground(2,QColor(s=="failed"||s=="timed_out"?(m_dark?"#ff8c93":"#b42332"):s=="unknown"?(m_dark?"#9aabb8":"#657582"):active(j)?(m_dark?"#e9b949":"#9b6b00"):(m_dark?"#81d8bf":"#237d67")));
    }
}
