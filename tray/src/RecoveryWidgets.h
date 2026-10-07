#pragma once
#include "HgsClient.h"
#include <QCheckBox>
#include <QComboBox>
#include <QDateTime>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QFrame>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QPlainTextEdit>
#include <QTimer>
#include <QVBoxLayout>
#include <cmath>
#include <functional>

namespace RecoveryUi {
inline QString status(const QJsonObject &job)
{
    const auto state = job.value("state").toString();
    if (state == "waiting") {
        const auto remaining = qMax(0, int(std::ceil(job.value("due_at").toDouble() - QDateTime::currentMSecsSinceEpoch()/1000.0)));
        return remaining > 0 ? QObject::tr("Retry in %1 s").arg(remaining) : QObject::tr("Waiting to retry");
    }
    if (state == "dispatching") return QObject::tr("Sending recovery request…");
    if (state == "retrying") return job.value("acknowledged").toBool() ? QObject::tr("Recovery in progress") : QObject::tr("Waiting for delivery confirmation…");
    if (state == "exhausted") return QObject::tr("Automatic attempts exhausted");
    if (state == "uncertain") return QObject::tr("Check the agent before retrying");
    if (state == "blocked") return QObject::tr("Automatic recovery needs attention");
    if (state == "cancelled") return QObject::tr("Automatic recovery cancelled");
    if (state == "succeeded") return QObject::tr("Recovered");
    return QObject::tr("Automatic recovery is not scheduled");
}

class Panel : public QFrame {
public:
    std::function<void(const QJsonObject &)> action;
    std::function<void()> openSettings;
    explicit Panel(QWidget *parent=nullptr):QFrame(parent) {
        setObjectName("recoveryPanel"); setFrameShape(QFrame::StyledPanel);setFocusPolicy(Qt::StrongFocus);
        auto *layout=new QVBoxLayout(this); layout->setContentsMargins(14,10,14,10); layout->setSpacing(8);
        m_status=new QLabel; m_status->setTextFormat(Qt::PlainText);m_status->setWordWrap(true); layout->addWidget(m_status);
        m_detail=new QLabel; m_detail->setTextFormat(Qt::PlainText);m_detail->setWordWrap(true); layout->addWidget(m_detail);
        auto *row=new QHBoxLayout; m_now=new QPushButton(tr("Retry now"));m_now->setObjectName("recoveryNow");m_cancel=new QPushButton(tr("Cancel retry"));m_cancel->setObjectName("recoveryCancel");
        m_history=new QPushButton(tr("Attempts…"));m_history->setObjectName("recoveryHistory");
        auto *settings=new QPushButton(tr("Settings…")); row->addWidget(m_now);row->addWidget(m_cancel);row->addWidget(m_history);row->addStretch();row->addWidget(settings);layout->addLayout(row);
        connect(m_history,&QPushButton::clicked,this,[this]{
            QDialog dialog(this);dialog.setObjectName("recoveryHistoryDialog");dialog.setWindowTitle(tr("Recovery attempts — hgs zerus"));dialog.resize(560,360);
            auto *layout=new QVBoxLayout(&dialog);layout->setContentsMargins(20,20,20,20);layout->setSpacing(12);
            auto *history=new QPlainTextEdit;history->setReadOnly(true);QStringList entries;
            for(const auto &value:m_job.value("history").toArray()) {
                const auto entry=value.toObject();const auto state=entry.value("state").toString();
                const auto label=state=="waiting"?tr("Retry scheduled"):RecoveryUi::status(entry);
                entries<<tr("%1  %2 (%3 attempts)").arg(QDateTime::fromMSecsSinceEpoch(qint64(entry.value("at").toDouble()*1000)).toLocalTime().toString("d MMM HH:mm:ss"),label).arg(entry.value("attempt").toInt());
                if(!entry.value("reason").toString().isEmpty())entries<<entry.value("reason").toString();
            }
            history->setPlainText(entries.join('\n'));layout->addWidget(history);
            auto *buttons=new QDialogButtonBox(QDialogButtonBox::Close);layout->addWidget(buttons);connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);dialog.exec();
        });
        connect(settings,&QPushButton::clicked,this,[this]{if(openSettings)openSettings();});
        for(auto *button:{m_now,m_cancel})connect(button,&QPushButton::clicked,this,[this,button]{
            if(!action)return; m_pending=true;refresh(); action({{"name",m_job.value("name")},{"id",m_job.value("id")},{"action",button==m_now?"now":"cancel"}});
        });
        auto *timer=new QTimer(this);connect(timer,&QTimer::timeout,this,[this]{if(isVisible())refresh();});timer->start(1000);hide();
    }
    void setState(const QJsonObject &details,bool online) {
        const auto job=details.value("recovery").toObject();
        if(job.value("id")!=m_job.value("id")) {m_pending=false;m_error.clear();}
        m_job=job;m_online=online;
        setVisible(!job.isEmpty() && job.value("state")!="succeeded"); refresh();
    }
    void finished(const QString &error) { m_pending=false;m_error=error;refresh(); }
private:
    void refresh() {
        const auto state=m_job.value("state").toString(); const bool waiting=state=="waiting";
        if(!waiting&&(m_now->hasFocus()||m_cancel->hasFocus()))setFocus(Qt::OtherFocusReason);
        const auto delays=m_job.value("delays").toArray();const bool infinite=!delays.isEmpty()&&delays.last().toInt()==0;
        const int count=int(delays.size())-(!delays.isEmpty()&&delays.last().toInt()<=0?1:0);
        m_status->setText(RecoveryUi::status(m_job)+(infinite?tr(" (%1 attempts, repeats until stopped)").arg(m_job.value("attempt").toInt()):tr(" (%1/%2 attempts)").arg(m_job.value("class_attempt").toInt(m_job.value("attempt").toInt())).arg(count)));
        m_detail->setText(!m_error.isEmpty()?m_error:(!m_job.value("reason").toString().isEmpty()?m_job.value("reason").toString():
            (m_job.value("action")=="retry_request"?tr("Retry the failed provider request"):tr("Send a continuation message in this conversation"))));
        m_now->setVisible(waiting);m_cancel->setVisible(waiting);
        m_history->setEnabled(!m_job.value("history").toArray().isEmpty());
        m_now->setEnabled(waiting&&m_online&&!m_pending&&m_job.value("not_before").toDouble()<=QDateTime::currentMSecsSinceEpoch()/1000.0);
        m_cancel->setEnabled(waiting&&m_online&&!m_pending);
    }
    QJsonObject m_job; bool m_online=false,m_pending=false; QString m_error;
    QLabel *m_status,*m_detail; QPushButton *m_now,*m_cancel,*m_history;
};
}
