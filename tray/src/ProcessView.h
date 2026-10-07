#pragma once
#include <QWidget>
#include <QJsonObject>
#include <QHash>
#include <QTimer>
#include <QElapsedTimer>
#include <QSet>

class HgsClient;
class QTreeWidget;
class QTreeWidgetItem;
class QPlainTextEdit;
class QLabel;
class QPushButton;
class BusyIndicator;

class ProcessView : public QWidget {
    Q_OBJECT
public:
    explicit ProcessView(HgsClient *client,QWidget *parent=nullptr);
    void setSession(const QString &host,const QString &name,const QString &archive,const QJsonObject &inspection,bool online,bool live);
    void setTheme(bool dark);
    void applyPreferences();
    void selectProcess(const QString &id);
signals:
    void refreshRequested();
protected:
    void showEvent(QShowEvent *event) override;
    void hideEvent(QHideEvent *event) override;
private:
    void select();
    void requestOutput(bool force=false);
    void stopSelected();
    void stopNext();
    void updateActions();
    void updateBusy();
    QStringList selectedIds() const;
    void refreshDurations();
    void saveView();
    HgsClient *m_client;
    QTreeWidget *m_list;
    QTreeWidgetItem *m_active,*m_finished,*m_liveProcesses;
    QPlainTextEdit *m_output;
    QLabel *m_summary,*m_details,*m_notice,*m_source,*m_stopNotice;
    QPushButton *m_stop,*m_copy;
    BusyIndicator *m_busy;
    QTimer m_timer,m_busyDelay;
    QElapsedTimer m_outputAge;
    QString m_host,m_name,m_archive,m_run,m_conversation,m_generation,m_key,m_selected,m_requestProcess;
    QHash<QString,QJsonObject> m_jobs;
    QHash<QString,QTreeWidgetItem*> m_rows;
    struct View {QString selected,output;QStringList selection;int scroll=0;int listScroll=0;};
    QHash<QString,View> m_views;
    quint64 m_request=0;
    quint64 m_stopRequest=0;
    QStringList m_stopQueue,m_stopErrors;
    QString m_stopId,m_stopKey;
    QSet<QString> m_stopPending;
    int m_stopSent=0,m_stopSkipped=0;
    bool m_online=false,m_live=false,m_dark=true,m_outputDirty=false;
};
