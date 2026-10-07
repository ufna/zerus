#pragma once
#include "FleetState.h"
#include <QQueue>
#include <functional>

struct AccountUsageRef {
    QString host, id, provider, home, label, session, run, authRevision;
    QString key() const { return host+'\n'+id+'\n'+home+(id.isEmpty()?'\n'+session+'\n'+run:QString()); }
    bool valid() const { return !id.isEmpty() || !session.isEmpty(); }
    static AccountUsageRef profile(const QJsonObject &p) {
        return {p["host"].toString(),p["id"].toString(),p["provider"].toString(),p["home"].toString(),p["label"].toString(),{}, {},p["auth_revision"].toString()};
    }
    static AccountUsageRef bound(const QString &host, const SessionInfo &s) {
        return {host,s.accountId,s.cmd,s.accountHome,{},s.name,s.runId};
    }
};

// One snapshot per machine, profile and agent home. Navigation never clears it.
class AccountUsageStore : public QObject {
    Q_OBJECT
public:
    static constexpr qint64 RefreshInterval = 5 * 60 * 1000;
    explicit AccountUsageStore(const QString &hgs, QObject *parent = nullptr, std::function<qint64()> clock = {});
    void setFleet(const FleetState &fleet) { m_fleet=fleet; }
    void ensure(const AccountUsageRef &ref, bool force = false);
    QJsonObject data(const AccountUsageRef &ref) const;
signals:
    void changed(const QString &key);
private:
    struct Entry { AccountUsageRef ref; QJsonObject data; qint64 attempted=0; bool pending=false, failed=false; };
    void dispatch();
    void finish(quint64 request, const QJsonObject &data);
    bool online(const QString &host) const;
    HgsClient m_client;
    FleetState m_fleet;
    QHash<QString,Entry> m_entries;
    QHash<quint64,AccountUsageRef> m_pending;
    QQueue<QString> m_queue;
    std::function<qint64()> m_now;
};
