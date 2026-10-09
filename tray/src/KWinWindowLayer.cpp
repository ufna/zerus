#include "KWinWindowLayer.h"
#include <QCoreApplication>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QUuid>

namespace {
const QString service = QStringLiteral("org.kde.KWin");
const QString scripting = QStringLiteral("org.kde.kwin.Scripting");
QDBusMessage call(const QString &path, const QString &interface, const QString &method) {
    return QDBusMessage::createMethodCall(service, path, interface, method);
}
}

KWinWindowLayer::KWinWindowLayer(QObject *parent, const QDBusConnection &bus)
    : QObject(parent), m_bus(bus), m_timeout(this)
{
    m_path = QStringLiteral("/org/hgdev/Zerus/WindowLayer/") + QUuid::createUuid().toString(QUuid::Id128);
    m_registered = m_bus.registerObject(m_path, this, QDBusConnection::ExportAllSlots);
    m_timeout.setSingleShot(true); m_timeout.setInterval(3000);
    connect(&m_timeout, &QTimer::timeout, this, [this] { complete({}, false, "unavailable"); });
}

KWinWindowLayer::~KWinWindowLayer()
{
    cancel();
    if (m_registered) m_bus.unregisterObject(m_path);
}

QString KWinWindowLayer::appId() const
{
    const auto desktop = QGuiApplication::desktopFileName();
    return desktop.isEmpty() ? QCoreApplication::applicationName() : desktop;
}

bool KWinWindowLayer::begin()
{
    if (m_busy) return false;
    m_owner = m_bus.interface() ? m_bus.interface()->serviceOwner(service).value() : QString();
    if (!m_registered || m_owner.isEmpty()) { emit finished({}, false, "unavailable"); return false; }
    m_busy = true; m_token = QUuid::createUuid().toString(QUuid::Id128); m_timeout.start();
    return true;
}

QString KWinWindowLayer::script(const QJsonObject &config)
{
    // JSON serialization keeps captions, application IDs and paths literal.
    return "const config = " + QString::fromUtf8(QJsonDocument(config).toJson(QJsonDocument::Compact)) + R"JS(;
(function () {
    let id = "", above = false, error = "unsupported";
    try {
        const matches = workspace.windowList().filter(function (w) {
            return w.pid === config.pid && w.normalWindow && !w.dialog && !w.deleted
                && String(w.resourceClass) === config.appId
                && (config.id ? String(w.internalId) === config.id : w.caption === config.caption);
        });
        error = matches.length === 0 ? "missing" : "ambiguous";
        if (matches.length === 1) {
            const w = matches[0];
            if (typeof w.keepAbove === "boolean" && w.internalId) {
                id = String(w.internalId);
                if (typeof config.desired === "boolean") w.keepAbove = config.desired;
                above = w.keepAbove;
                error = typeof config.desired === "boolean" && above !== config.desired ? "denied" : "";
            } else error = "unsupported";
        }
    } catch (_) { error = "unsupported"; }
    callDBus(config.service, config.path, "org.hgdev.Zerus.WindowLayer", "Report",
        config.token, id, above, error);
    callDBus("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "unloadScript", config.plugin);
})();
)JS";
}

void KWinWindowLayer::request(const QString &caption, const QString &id, std::optional<bool> on)
{
    if (!begin()) return;
    m_plugin = "zerus-window-layer-" + m_token;
    clearFile();
    if (!m_file.open()) { complete({}, false, "unavailable"); return; }
    const QJsonObject config{{"pid", QCoreApplication::applicationPid()}, {"appId", appId()},
        {"caption", caption}, {"id", id}, {"desired", on ? QJsonValue(*on) : QJsonValue()},
        {"service", m_bus.baseService()}, {"path", m_path}, {"token", m_token}, {"plugin", m_plugin}};
    const auto bytes = script(config).toUtf8();
    if (m_file.write(bytes) != bytes.size() || !m_file.flush()) { complete({}, false, "unavailable"); return; }
    auto message = call("/Scripting", scripting, "loadScript");
    message.setArguments({m_file.fileName(), m_plugin});
    const auto token = m_token;
    auto *watcher = new QDBusPendingCallWatcher(m_bus.asyncCall(message, 2000), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, token](auto *watcher) {
        const QDBusPendingReply<int> reply = *watcher; watcher->deleteLater();
        if (!m_busy || token != m_token) return;
        if (reply.isError() || reply.value() < 0) { complete({}, false, "unavailable"); return; }
        auto run = call("/Scripting/Script" + QString::number(reply.value()), "org.kde.kwin.Script", "run");
        auto *running = new QDBusPendingCallWatcher(m_bus.asyncCall(run, 2000), this);
        connect(running, &QDBusPendingCallWatcher::finished, this, [this, token](auto *running) {
            const QDBusPendingReply<> reply = *running; running->deleteLater();
            if (m_busy && token == m_token && reply.isError()) complete({}, false, "unavailable");
        });
    });
}

void KWinWindowLayer::Report(const QString &token, const QString &id, bool on, const QString &error)
{
    if (!calledFromDBus() || message().service() != m_owner || token != m_token || !m_busy) return;
    if (!id.isEmpty() && QUuid(id).isNull()) { complete({}, false, "unsupported"); return; }
    complete(id, on, error);
}

void KWinWindowLayer::inspect(const QString &id)
{
    if (!begin()) return;
    auto message = call("/KWin", service, "getWindowInfo"); message.setArguments({id});
    const auto token = m_token;
    auto *watcher = new QDBusPendingCallWatcher(m_bus.asyncCall(message, 2000), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, token, id](auto *watcher) {
        const QDBusPendingReply<QVariantMap> reply = *watcher; watcher->deleteLater();
        if (!m_busy || token != m_token) return;
        const auto info = reply.value();
        if (reply.isError()) { complete({}, false, "unavailable"); return; }
        if (info.value("uuid").toString() != id || info.value("pid").toLongLong() != QCoreApplication::applicationPid()
            || info.value("resourceClass").toString() != appId()) { complete({}, false, "missing"); return; }
        if (info.value("keepAbove").metaType().id() != QMetaType::Bool) { complete({}, false, "unsupported"); return; }
        complete(id, info.value("keepAbove").toBool(), {});
    });
}

void KWinWindowLayer::unload()
{
    if (m_plugin.isEmpty()) return;
    auto message = call("/Scripting", scripting, "unloadScript"); message.setArguments({m_plugin});
    m_bus.asyncCall(message, 1000); m_plugin.clear();
}

void KWinWindowLayer::cancel()
{
    m_timeout.stop(); m_busy = false; m_token.clear(); unload(); clearFile();
}

void KWinWindowLayer::clearFile()
{
    if (!m_file.isOpen()) return;
    m_file.close(); m_file.remove();
}

void KWinWindowLayer::complete(const QString &id, bool on, const QString &error)
{
    m_timeout.stop(); m_busy = false; unload(); clearFile();
    emit finished(id, on, error);
}
