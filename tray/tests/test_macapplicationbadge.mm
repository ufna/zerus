#include <QtTest>
#include "../src/MacApplicationBadge.mm"

// A detached tile exercises AppKit's badge property without touching NSApp's
// live Dock tile, requesting notification permission or activating the app.
@interface HgsBadgeTestTile : NSDockTile
@property NSInteger writes;
@end
@implementation HgsBadgeTestTile
- (void)setBadgeLabel:(NSString *)label { self.writes++; [super setBadgeLabel:label]; }
@end

class TestMacApplicationBadge : public QObject {
    Q_OBJECT
private slots:
    void nativeBadgeCountClearingAndShutdown() {
        HgsBadgeTestTile *tile = [HgsBadgeTestTile new];
        auto badge = std::make_unique<MacApplicationBadge>(tile);
        badge->setCount(3); QCOMPARE(QString::fromNSString(tile.badgeLabel), QString("3"));
        const auto writes = tile.writes;
        badge->setCount(3); QCOMPARE(tile.writes, writes);
        badge->setCount(123); QCOMPARE(QString::fromNSString(tile.badgeLabel), QString("123"));
        badge->setCount(0); QVERIFY(tile.badgeLabel == nil || tile.badgeLabel.length == 0);
        const auto cleared = tile.writes;
        badge->setCount(-5); QCOMPARE(tile.writes, cleared);
        badge->setCount(1); badge.reset(); QVERIFY(tile.badgeLabel == nil || tile.badgeLabel.length == 0);
    }
};
QTEST_GUILESS_MAIN(TestMacApplicationBadge)
#include "test_macapplicationbadge.moc"
