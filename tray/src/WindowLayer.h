#pragma once
#include <QObject>
#include <QTimer>
#include <QWidget>
#include <optional>

class KWinWindowLayer;

// Wayland requests need confirmed compositor state, not just a Qt window flag.
class WindowLayer : public QObject {
    Q_OBJECT
public:
    explicit WindowLayer(QWidget *window);
    bool supported() const { return m_supported; }
    bool onTop() const { return m_onTop; }
    bool busy() const { return m_busy; }
    QString hint() const;
    void request(bool on);
signals:
    void changed();
protected:
    bool eventFilter(QObject *object, QEvent *event) override;
private:
    void probe();
    void applyQt(bool on);
    QWidget *m_window;
    bool m_wayland, m_supported = false, m_onTop = false, m_busy = false;
    bool m_restore = true, m_desired = false;
    bool m_polling = false;
    std::optional<bool> m_queued;
    int m_attempts = 0, m_epoch = 0;
    QString m_id, m_error;
    QTimer m_poll;
    KWinWindowLayer *m_kwin = nullptr;
};
