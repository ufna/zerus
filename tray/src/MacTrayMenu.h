#pragma once

class QMenu;

// Called synchronously from QSystemTrayIcon's Context activation, while AppKit's
// current event still identifies the clicked status item. Qt keeps its delegate.
void showMacTrayMenu(QMenu *menu);
