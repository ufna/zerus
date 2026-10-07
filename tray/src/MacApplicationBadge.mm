#include "ApplicationBadge.h"
#include <QString>
#import <AppKit/AppKit.h>

class MacApplicationBadge : public ApplicationBadge {
public:
    explicit MacApplicationBadge(NSDockTile *tile = nil) : m_tile(tile ?: NSApp.dockTile) {}
    ~MacApplicationBadge() override { if (m_count > 0) m_tile.badgeLabel = nil; }
    void setCount(int count) override {
        count = qMax(0, count);
        if (m_count == count) return;
        m_count = count;
        m_tile.badgeLabel = count > 0 ? QString::number(count).toNSString() : nil;
    }
private:
    NSDockTile *m_tile;
    int m_count = -1;
};

std::unique_ptr<ApplicationBadge> makeApplicationBadge() { return std::make_unique<MacApplicationBadge>(); }
