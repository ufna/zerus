#pragma once

#include <QObject>
#include <functional>

class QApplication;

// A menu-bar app becomes a normal macOS app while it owns an open window.
// Popups and tooltips do not count; minimized/hidden-by-Cmd-H windows do.
class MacAppVisibility : public QObject {
public:
    explicit MacAppVisibility(QApplication &app, std::function<void(bool)> applyPolicy = {});
protected:
    bool eventFilter(QObject *object, QEvent *event) override;
private:
    void scheduleUpdate();
    void update();
    std::function<void(bool)> m_applyPolicy;
    bool m_pending = false, m_regular = false, m_reopen = false;
};
