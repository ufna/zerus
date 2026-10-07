#pragma once
#include "ApplicationBadge.h"
#include <QDBusConnection>
#include <QTimer>
#include <QVariantMap>

class LinuxApplicationBadge : public ApplicationBadge {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "com.canonical.Unity.LauncherEntry")
public:
    explicit LinuxApplicationBadge(const QDBusConnection &bus = QDBusConnection::sessionBus());
    ~LinuxApplicationBadge() override;
    void setCount(int count) override;
public slots:
    QString Query(QVariantMap &properties) const;
private:
    void publish();
    QVariantMap properties() const;
    QDBusConnection m_bus;
    QTimer m_republish;
    int m_count = 0;
    bool m_sent = false, m_registered = false;
};
