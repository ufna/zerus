#pragma once
#include <QGuiApplication>
#include <QSettings>
#include <QWidget>
#include <QWindow>

// Optionally keeps the Zerus window above other windows. X11 maps the hint to
// _NET_WM_STATE_ABOVE and macOS to a floating window level; Wayland lets no
// application place itself above others.
namespace WindowLayer {
inline bool supported() { return QGuiApplication::platformName() != QLatin1String("wayland"); }
inline bool alwaysOnTop() { return supported() && QSettings().value("workspace/alwaysOnTop", false).toBool(); }

// QWidget::setWindowFlag would hide a visible window. The native window takes the
// hint in place, and the widget keeps the same flags for a later re-creation.
inline void apply(QWidget *window, bool onTop)
{
    auto flags = window->windowFlags(); flags.setFlag(Qt::WindowStaysOnTopHint, onTop);
    if (flags == window->windowFlags()) return;
    window->overrideWindowFlags(flags);
    if (auto *handle = window->windowHandle()) handle->setFlags(flags);
}
}
