#pragma once

#include <KStatusNotifierItem>

#include "TrayIcon.h"

// Plasma delivers primary clicks through Activate. Context menus remain hosted by
// the panel over DBusMenu: QMenu::popup() cannot grab input from a tray on Wayland.
// ItemIsMenu must be false so primary clicks reach activateRequested.
//
// Собственность. KStatusNotifierItem::setContextMenu() БЕРЁТ меню в собственность и
// удаляет его в своём деструкторе (kstatusnotifieritem.cpp, ~KStatusNotifierItem:
// `delete d->menu`); отобрать назад нельзя -- setContextMenu(nullptr) тоже удаляет.
// Поэтому m_item обязан умереть РАНЬШЕ TrayMenu (порядок членов TrayAgent.h), а TrayMenu
// держит меню через QPointer и после нас delete'ит nullptr -- см. ~TrayMenu.
class SniTrayIcon : public TrayIcon {
public:
    SniTrayIcon();
    void setIcon(const QIcon &icon) override;
    void setToolTip(const QString &text) override;
    void setContextMenu(QMenu *menu) override;
    void showMessage(const QString &title, const QString &body,
                     QSystemTrayIcon::MessageIcon icon, int msecs) override;
    void show() override;

    // Для теста (tests/test_snitrayicon.cpp): три настройки конструктора, без которых
    // иконки либо не видно, либо ЛКМ по ней ничего не делает, либо в меню чужой Quit.
    const KStatusNotifierItem &item() const { return m_item; }

private:
    KStatusNotifierItem m_item;
};
