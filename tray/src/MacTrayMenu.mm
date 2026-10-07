#include "MacTrayMenu.h"

#include <QMenu>
#import <AppKit/AppKit.h>

void showMacTrayMenu(QMenu *menu)
{
    NSMenu *nativeMenu = menu->toNSMenu();
    if (!nativeMenu)
        return;
    // Keep Qt's NSMenu delegate: it propagates aboutToShow/aboutToHide and actions.
    // Those signals also prevent icon updates during menu tracking (QuietTrayIcon).
    NSEvent *event = NSApp.currentEvent;
    NSView *view = event.window.contentView;
    if (view) {
        [NSMenu popUpContextMenu:nativeMenu withEvent:event forView:view];
    } else {
        [nativeMenu popUpMenuPositioningItem:nil atLocation:NSEvent.mouseLocation inView:nil];
    }
}
