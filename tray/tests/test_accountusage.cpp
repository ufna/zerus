#include <QtTest>
#include "AccountUsageStore.h"
#include "AccountUsage.h"
#include "CacheStatus.h"

class TestAccountUsage : public QObject {
    Q_OBJECT
private slots:
    void savedCredentialsAreDistinctFromSignedOut() {
        for(const auto &status:{"credentials_locked","desktop_session_unavailable","credentials_unavailable"}) {
            const auto text=AccountUsage::state({{"status",status},{"provider","claude"}});
            QVERIFY(!text.contains("No native subscription"));
            QVERIFY(text.contains("Keychain") || text.contains("desktop"));
        }
    }
    void expiredLimitsAndUnknownCacheNeverImplyAvailableCapacity() {
        QJsonObject window{{"window_minutes",300},{"used_percent",100},{"resets_at",QDateTime::currentSecsSinceEpoch()+3600}};
        QJsonObject data{{"windows",QJsonArray{window}}};
        QVERIFY(AccountUsage::windowText(window).contains("1h"));QCOMPARE(AccountUsage::highest(data),100);
        QVERIFY(AccountUsage::exhausted(data).contains("Resets in"));
        window.remove("resets_at");data["windows"]=QJsonArray{window};
        QVERIFY(AccountUsage::exhausted(data).contains("unavailable"));QCOMPARE(AccountUsage::highest(data),100);
        window["resets_at"]=QDateTime::currentSecsSinceEpoch()-60;data["windows"]=QJsonArray{window};
        QVERIFY(AccountUsage::windowText(window).contains("refresh"));QCOMPARE(AccountUsage::highest(data),-1);
        QVERIFY(AccountUsage::exhausted(data).contains("refresh"));
        QJsonObject session{{"session_usage",QJsonObject{{"status","ok"},{"prompt_cache",QJsonObject{{"status","unknown"},{"cache_read",10000}}}}}};
        QVERIFY(!CacheStatus::expired(session));QVERIFY(CacheStatus::warning(session).isEmpty());
        session["cache_hint"]=QJsonObject{{"status","saving_hint"},{"tokens",484800}};
        QVERIFY(!CacheStatus::expired(session));QVERIFY(CacheStatus::warning(session).contains("still be warm"));
        session["cache_hint"]=QJsonObject{{"status","cold"},{"tokens",756000}};
        QVERIFY(CacheStatus::expired(session));QVERIFY(CacheStatus::warning(session).contains("cold cache"));
    }
    void cacheIsSharedByAccountWithTimedAndManualRefresh() {
        qint64 now=1000;AccountUsageStore store("/nonexistent/hgs",nullptr,[&]{return now;});
        auto *client=store.findChild<HgsClient *>();
        AccountUsageRef a{{},"work","codex","/accounts/work","Work","codex/a","run-a"};auto b=a;b.session="codex/b";b.run="run-b";
        const QJsonObject data{{"id","work"},{"home","/accounts/work"},{"provider","codex"},{"status","ok"},{"run_id","run-a"},{"identity",QJsonObject{{"email","work@example.test"}}},{"windows",QJsonArray{QJsonObject{{"used_percent",78},{"window_minutes",300}}}}};
        store.ensure(a);QVERIFY(store.data(a)["refreshing"].toBool());
        client->accountsReady(1,{},data);QCOMPARE(store.data(a)["windows"],data["windows"]);
        store.ensure(b);QVERIFY(!store.data(b)["refreshing"].toBool());QCOMPARE(store.data(b)["windows"],data["windows"]);
        now+=AccountUsageStore::RefreshInterval-1;store.ensure(b);QVERIFY(!store.data(b)["refreshing"].toBool());
        ++now;store.ensure(b);QVERIFY(store.data(b)["refreshing"].toBool());QCOMPARE(store.data(b)["windows"],data["windows"]);
        client->accountsFailed(2,{},"offline");QVERIFY(store.data(b)["refresh_error"].toBool());QCOMPARE(store.data(b)["windows"],data["windows"]);
        store.ensure(b);QVERIFY(!store.data(b)["refreshing"].toBool());
        store.ensure(b,true);store.ensure(a,true);QVERIFY(store.data(b)["refreshing"].toBool());
        auto fresh=data;fresh["run_id"]="run-b";client->accountsReady(3,{},fresh);QVERIFY(!store.data(a)["refreshing"].toBool());QVERIFY(!store.data(a)["refresh_error"].toBool());
        AccountUsageRef catalog=a;catalog.session.clear();catalog.run.clear();store.ensure(catalog);QVERIFY(!store.data(catalog)["refreshing"].toBool());
        auto remote=catalog;remote.host="mac";store.ensure(remote);QVERIFY(store.data(remote)["windows"].toArray().isEmpty());
        auto otherHome=catalog;otherHome.home="/different/home";store.ensure(otherHome);QVERIFY(store.data(otherHome)["windows"].toArray().isEmpty());
        client->accountsReady(4,"mac",fresh);QCOMPARE(store.data(remote)["windows"],fresh["windows"]);
        client->accountsReady(5,{},fresh);QVERIFY(store.data(otherHome)["windows"].toArray().isEmpty());
    }
    void credentialChangeRefreshesIdentityWithoutWaitingFiveMinutes() {
        AccountUsageStore store("/nonexistent/hgs");auto *client=store.findChild<HgsClient *>();
        AccountUsageRef ref{{},"work","claude","/accounts/work","Work",{},{},"before"};
        store.ensure(ref);client->accountsReady(1,{},{{"id","work"},{"home",ref.home},{"provider","claude"},{"status","signed_out"}});
        store.ensure(ref);QVERIFY(!store.data(ref)["refreshing"].toBool());
        ref.authRevision="after";store.ensure(ref);QVERIFY(store.data(ref)["refreshing"].toBool());
        client->accountsReady(2,{},{{"id","work"},{"home",ref.home},{"provider","claude"},{"status","ok"},{"identity",QJsonObject{{"email","work@example.test"}}}});
        QCOMPARE(store.data(ref)["status"].toString(),QString("ok"));
    }
    void refreshButtonShowsMotionUntilComplete() {
        AccountUsage::RefreshButton button;button.show();button.setRefreshing(true);
        const auto before=button.grab().toImage();QTest::qWait(100);QVERIFY(button.isRefreshing());QVERIFY(button.grab().toImage()!=before);
        button.setRefreshing(false);QVERIFY(!button.isRefreshing());QVERIFY(!button.icon().isNull());QVERIFY(button.toolTip().contains("Refresh"));
    }
    void footerExpandsOnlyForExtraText() {
        AccountUsage::Button button;button.show();
        QJsonObject first{{"used_percent",69},{"window_minutes",300},{"resets_at",QDateTime::currentSecsSinceEpoch()+3600}};
        const QJsonObject second{{"used_percent",23},{"window_minutes",10080}};
        auto setUsage=[&]{button.setData({{"status","ok"},{"provider","claude"},{"windows",QJsonArray{first,second}}});};
        const auto directory=qEnvironmentVariable("HGS_DASHBOARD_PREVIEW");
        auto preview=[&](const QString &name){if(!directory.isEmpty()){QDir().mkpath(directory);QVERIFY(button.grab().save(directory+"/quota-"+name+".png"));}};
        setUsage();const auto size=button.size();QVERIFY(size.width()<300);QCOMPARE(size.height(),22);preview("normal");
        first["used_percent"]=9;setUsage();QCOMPARE(button.size(),size);
        first["used_percent"]=100;setUsage();QVERIFY(button.width()>size.width());QCOMPARE(button.height(),size.height());preview("exhausted");
        first["used_percent"]=69;setUsage();QCOMPARE(button.size(),size);
        button.setData({{"status","unavailable"}});QVERIFY(button.width()<size.width());QCOMPARE(button.height(),size.height());
    }
};
QTEST_MAIN(TestAccountUsage)
#include "test_accountusage.moc"
