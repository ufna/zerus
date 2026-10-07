#include "LinuxApplicationBadge.h"
#include <QDBusMessage>
#include <QDBusServiceWatcher>

namespace {
const QString path = QStringLiteral("/org/hgdev/Zerus/LauncherEntry");
const QString launcher = QStringLiteral("application://hgs-tray.desktop");
}

LinuxApplicationBadge::LinuxApplicationBadge(const QDBusConnection &bus) : m_bus(bus), m_republish(this)
{
    m_registered = m_bus.registerObject(path, this, QDBusConnection::ExportAllSlots);
    m_republish.setSingleShot(true); m_republish.setInterval(1000);
    connect(&m_republish, &QTimer::timeout, this, &LinuxApplicationBadge::publish);
    // Shells can start after Zerus or restart without an attention-count change.
    auto *watcher = new QDBusServiceWatcher(QStringLiteral("com.canonical.Unity"),
        m_bus, QDBusServiceWatcher::WatchForRegistration, this);
    watcher->addWatchedService(QStringLiteral("org.kde.plasmashell"));
    connect(watcher, &QDBusServiceWatcher::serviceRegistered, this, [this] { m_republish.start(); });
}

LinuxApplicationBadge::~LinuxApplicationBadge()
{
    if (m_count > 0) { m_count = 0; publish(); }
    if (m_registered) m_bus.unregisterObject(path);
}

void LinuxApplicationBadge::setCount(int count)
{
    count = qMax(0, count);
    if (m_sent && m_count == count) return;
    m_count = count; publish();
}

QVariantMap LinuxApplicationBadge::properties() const
{
    return {{QStringLiteral("count"), QVariant::fromValue(qint64(m_count))},
        {QStringLiteral("count-visible"), m_count > 0}};
}

QString LinuxApplicationBadge::Query(QVariantMap &result) const
{
    result = properties(); return launcher;
}

void LinuxApplicationBadge::publish()
{
    auto message = QDBusMessage::createSignal(path, QStringLiteral("com.canonical.Unity.LauncherEntry"), QStringLiteral("Update"));
    message.setArguments({launcher, properties()}); m_sent = m_bus.send(message);
}

std::unique_ptr<ApplicationBadge> makeApplicationBadge() { return std::make_unique<LinuxApplicationBadge>(); }
