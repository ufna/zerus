#pragma once
#include <QDBusConnection>
#include <QDBusContext>
#include <QJsonObject>
#include <QObject>
#include <QTemporaryFile>
#include <QTimer>
#include <optional>

// One-shot scripts use KWin's window API, then unload themselves. No global
// window rules, shortcuts or Plasma-private Wayland interfaces are installed.
class KWinWindowLayer : public QObject, protected QDBusContext {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.hgdev.Zerus.WindowLayer")
public:
    explicit KWinWindowLayer(QObject *parent = nullptr,
        const QDBusConnection &bus = QDBusConnection::sessionBus());
    ~KWinWindowLayer() override;
    void request(const QString &caption, const QString &id, std::optional<bool> on);
    void inspect(const QString &id);
    void cancel();
    static QString script(const QJsonObject &config);
signals:
    void finished(const QString &id, bool on, const QString &error);
public slots:
    void Report(const QString &token, const QString &id, bool on, const QString &error);
private:
    bool begin();
    void complete(const QString &id, bool on, const QString &error);
    void unload();
    void clearFile();
    QString appId() const;
    QDBusConnection m_bus;
    QString m_path, m_owner, m_token, m_plugin;
    QTemporaryFile m_file;
    QTimer m_timeout;
    bool m_registered = false, m_busy = false;
};
