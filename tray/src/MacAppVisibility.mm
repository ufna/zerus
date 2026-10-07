#include "MacAppVisibility.h"

#include <QApplication>
#include <QDebug>
#include <QEvent>
#include <QTimer>
#include <QWidget>
#import <AppKit/AppKit.h>

namespace {
bool isAppWindow(const QWidget *widget)
{
    return widget->isWindow() && (widget->windowType() == Qt::Window
        || widget->windowType() == Qt::Dialog || widget->windowType() == Qt::Sheet);
}

void applyNativePolicy(bool regular)
{
    @autoreleasepool {
        if (![NSApp setActivationPolicy:regular ? NSApplicationActivationPolicyRegular
                                                : NSApplicationActivationPolicyAccessory]) {
            qWarning() << "Could not update Zerus macOS application visibility";
            return;
        }
        if (regular) {
            // Undo Qt's symbolic window-icon override. nil restores the native
            // bundle icon, including Icon Composer layers and system masking;
            // loading the ICNS alone would flatten it and omit larger renditions.
            NSApp.applicationIconImage = nil;
        }
    }
}
}

MacAppVisibility::MacAppVisibility(QApplication &app, std::function<void(bool)> applyPolicy)
    : QObject(&app), m_applyPolicy(applyPolicy ? std::move(applyPolicy) : applyNativePolicy)
{
    app.installEventFilter(this);
    scheduleUpdate();
}

bool MacAppVisibility::eventFilter(QObject *object, QEvent *event)
{
    if (object == qApp && event->type() == QEvent::ApplicationActivate) {
        // Qt forwards Dock reopen events here. Restore a minimized window
        // without replacing Qt's native application delegate.
        m_reopen = true;
        scheduleUpdate();
    } else if (event->type() == QEvent::Show || event->type() == QEvent::Hide
               || event->type() == QEvent::WindowStateChange || event->type() == QEvent::Destroy) {
        const auto *widget = qobject_cast<QWidget *>(object);
        if (widget && isAppWindow(widget)) scheduleUpdate();
    }
    return false;
}

void MacAppVisibility::scheduleUpdate()
{
    if (m_pending) return;
    m_pending = true;
    // Reconcile after show/close has completed, and coalesce dialog transitions.
    QTimer::singleShot(0, this, [this] { update(); });
}

void MacAppVisibility::update()
{
    m_pending = false;
    QWidget *minimized = nullptr;
    bool regular = false, visible = false;
    for (QWidget *widget : QApplication::topLevelWidgets()) {
        // Qt keeps isVisible() true for minimized windows and Cmd-H. A closed
        // window becomes false even when its windowState still says minimized.
        if (!isAppWindow(widget) || !widget->isVisible()) continue;
        regular = true;
        if (widget->isMinimized()) minimized = widget;
        else visible = true;
    }
    if (regular != m_regular) {
        m_regular = regular;
        m_applyPolicy(regular);
    }
    const bool reopen = m_reopen;
    m_reopen = false;
    if (reopen && minimized && !visible) {
        minimized->showNormal();
        minimized->raise();
        minimized->activateWindow();
    }
}
