#include <QtTest>
#include <QSignalSpy>
#include "../src/MacAttentionNotifier.mm"

// Exercise the native request/delegate boundary without posting to the user's
// Notification Center or changing their permissions during ctest.
@interface HgsFakeSettings : NSObject
@property UNAuthorizationStatus authorizationStatus;
@end
@implementation HgsFakeSettings
@end
@interface HgsFakeCenter : NSObject
@property(nonatomic, weak) id<UNUserNotificationCenterDelegate> delegate;
@property(nonatomic, strong) NSMutableArray<UNNotificationRequest *> *requests;
@property(nonatomic, strong) NSMutableArray<NSString *> *removed;
@property(nonatomic, copy) void (^settingsCallback)(UNNotificationSettings *);
@property(nonatomic, copy) void (^permissionCallback)(BOOL, NSError *);
@end
@implementation HgsFakeCenter
- (instancetype)init {
    self = [super init]; if (self) { self.requests = [NSMutableArray new]; self.removed = [NSMutableArray new]; } return self;
}
- (void)getNotificationSettingsWithCompletionHandler:(void (^)(UNNotificationSettings *))callback { self.settingsCallback = callback; }
- (void)requestAuthorizationWithOptions:(UNAuthorizationOptions)options completionHandler:(void (^)(BOOL, NSError *))callback {
    Q_UNUSED(options); self.permissionCallback = callback;
}
- (void)addNotificationRequest:(UNNotificationRequest *)request withCompletionHandler:(void (^)(NSError *))completion {
    [self.requests addObject:request]; completion(nil);
}
- (void)removePendingNotificationRequestsWithIdentifiers:(NSArray<NSString *> *)identifiers { [self.removed addObjectsFromArray:identifiers]; }
- (void)removeDeliveredNotificationsWithIdentifiers:(NSArray<NSString *> *)identifiers { [self.removed addObjectsFromArray:identifiers]; }
@end
@interface HgsFakeNotification : NSObject
@property(nonatomic, strong) UNNotificationRequest *request;
@end
@implementation HgsFakeNotification
@end
@interface HgsFakeResponse : NSObject
@property(nonatomic, strong) HgsFakeNotification *notification;
@property(nonatomic, copy) NSString *actionIdentifier;
@end
@implementation HgsFakeResponse
@end

class TestMacAttention : public QObject {
    Q_OBJECT
private slots:
    void nativeRequestsHaveIndividualTargets();
    void waitsForPermissionAndHonorsWithdrawal();
};
static void authorize(HgsFakeCenter *center, UNAuthorizationStatus status) {
    HgsFakeSettings *settings = [HgsFakeSettings new]; settings.authorizationStatus = status;
    center.settingsCallback((UNNotificationSettings *)settings);
}
void TestMacAttention::nativeRequestsHaveIndividualTargets()
{
    HgsFakeCenter *center = [HgsFakeCenter new];
    MacAttentionNotifier notifier((UNUserNotificationCenter *)center);
    QSignalSpy clicked(&notifier, &AttentionNotifier::activated);
    notifier.post("first", "Needs attention", "First session");
    notifier.post("second", "Needs attention", "Second session");
    QCOMPARE(center.requests.count, NSUInteger(0));
    authorize(center, UNAuthorizationStatusNotDetermined);
    QVERIFY(center.permissionCallback != nil); QCOMPARE(center.requests.count, NSUInteger(0));
    center.permissionCallback(YES, nil);
    QTRY_COMPARE(center.requests.count, NSUInteger(2));
    UNNotificationRequest *second = nil;
    for (UNNotificationRequest *request in center.requests) {
        QVERIFY(request.content.sound != nil);
        if ([request.content.userInfo[@"hgsAttentionToken"] isEqualToString:@"second"]) second = request;
    }
    QVERIFY(second != nil);
    HgsFakeResponse *response = [HgsFakeResponse new]; response.notification = [HgsFakeNotification new];
    response.notification.request = second; response.actionIdentifier = UNNotificationDefaultActionIdentifier;
    __block bool completed = false;
    [center.delegate userNotificationCenter:(UNUserNotificationCenter *)center
        didReceiveNotificationResponse:(UNNotificationResponse *)response withCompletionHandler:^{ completed = true; }];
    QVERIFY(completed); QTRY_COMPARE(clicked.size(), 1); QCOMPARE(clicked[0][0].toString(), QString("second"));
    response.actionIdentifier = UNNotificationDismissActionIdentifier;
    [center.delegate userNotificationCenter:(UNUserNotificationCenter *)center
        didReceiveNotificationResponse:(UNNotificationResponse *)response withCompletionHandler:^{}];
    QTest::qWait(20); QCOMPARE(clicked.size(), 1);
    notifier.withdraw("second"); QVERIFY([center.removed containsObject:second.identifier]);
}
void TestMacAttention::waitsForPermissionAndHonorsWithdrawal()
{
    HgsFakeCenter *center = [HgsFakeCenter new];
    auto notifier = std::make_unique<MacAttentionNotifier>((UNUserNotificationCenter *)center);
    QSignalSpy failed(notifier.get(), &AttentionNotifier::failed);
    notifier->post("gone", "Needs attention", "Already resolved"); notifier->withdraw("gone");
    authorize(center, UNAuthorizationStatusAuthorized);
    QTest::qWait(20); QCOMPARE(center.requests.count, NSUInteger(0));
    notifier->post("denied", "Needs attention", "Waiting"); authorize(center, UNAuthorizationStatusDenied);
    QTRY_COMPARE(failed.size(), 1); QCOMPARE(center.requests.count, NSUInteger(0));
    notifier->post("destroyed", "Needs attention", "Waiting");
    notifier.reset(); authorize(center, UNAuthorizationStatusAuthorized);
    QTest::qWait(20); QCOMPARE(center.requests.count, NSUInteger(0));
    QVERIFY(center.delegate == nil);
}
QTEST_GUILESS_MAIN(TestMacAttention)
#include "test_macattention.moc"
