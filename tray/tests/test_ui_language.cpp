#include <QtTest>
#include <QTemporaryDir>
#include "SettingsPage.h"
#include "UiLanguage.h"
#include "SessionPresentation.h"

class TestUiLanguage : public QObject {
    Q_OBJECT
private slots:
    void translatedSettingsAndPersistentChoice() {
        QTemporaryDir preferences;
        QVERIFY(preferences.isValid());
        QCoreApplication::setOrganizationName("zerus-language-test");
        QCoreApplication::setApplicationName("isolated");
        QSettings::setDefaultFormat(QSettings::IniFormat);
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, preferences.path());
        QSettings().setValue("workspace/language", "ru");
        QCOMPARE(zerusUiLanguage(), QString("ru"));
        ZerusTranslator translator;
        QTranslator qtTranslator;
        QVERIFY(installZerusTranslation(*qApp, translator, qtTranslator));
        QCOMPARE(QObject::tr("Accounts"), QString::fromUtf8("Учётные записи"));
        QCOMPARE(QCoreApplication::translate("SessionsWindow", "New session"), QString::fromUtf8("Новая сессия"));
        QCOMPARE(QCoreApplication::translate("UnknownInheritedContext", "Settings"), QString::fromUtf8("Настройки"));
        QVERIFY(translator.translate("UnknownInheritedContext", "Settings", "different meaning").isEmpty());
        QVERIFY(QCoreApplication::translate("QObject", "Project sync connected to %n computer(s)", nullptr, 1).contains(QString::fromUtf8("1 компьютер")));
        QVERIFY(QCoreApplication::translate("QObject", "Project sync connected to %n computer(s)", nullptr, 2).contains(QString::fromUtf8("2 компьютера")));
        QVERIFY(QCoreApplication::translate("QObject", "Project sync connected to %n computer(s)", nullptr, 5).contains(QString::fromUtf8("5 компьютеров")));
        QVERIFY(QCoreApplication::translate("QObject", "%1 / %2 tokens").contains("%1"));
        SessionInfo session; session.state="running"; session.processState="running";
        session.activity="idle"; session.activitySummary="Ready";
        QCOMPARE(SessionPresentation::currentAction(session), QString::fromUtf8("Готов"));
        session.activitySummary="custom tool output";
        QCOMPARE(SessionPresentation::currentAction(session), QString("custom tool output"));
        {
            SettingsPage settings("/nonexistent/zerus-test-hgs");
            settings.resize(1080, 900);
            settings.show();
            QTest::qWait(50);
            auto *language = settings.findChild<QComboBox *>("workspaceLanguage");
            QVERIFY(language);
            QCOMPARE(language->count(), 3);
            QCOMPARE(language->currentData().toString(), QString("ru"));
            QCOMPARE(language->itemText(0), QString::fromUtf8("Как в системе"));
            auto *sections = settings.findChild<QListWidget *>("settingsSections");
            QVERIFY(sections);
            for (int row=0; row<sections->count(); ++row)
                QVERIFY(sections->viewport()->width() >= sections->fontMetrics().horizontalAdvance(sections->item(row)->text())+12);
            const QString screenshot = qEnvironmentVariable("ZERUS_TRANSLATION_SCREENSHOT");
            if (!screenshot.isEmpty()) QVERIFY(settings.grab().save(screenshot));
            language->setCurrentIndex(language->findData("en"));
            QCOMPARE(QSettings().value("workspace/language").toString(), QString("en"));
        }
        qApp->removeTranslator(&translator);
        qApp->removeTranslator(&qtTranslator);
        QCOMPARE(zerusUiLanguage(), QString("en"));
        QCOMPARE(QObject::tr("Accounts"), QString("Accounts"));
        {
            SettingsPage settings("/nonexistent/zerus-test-hgs");
            auto *language = settings.findChild<QComboBox *>("workspaceLanguage");
            QCOMPARE(language->currentData().toString(), QString("en"));
            language->setCurrentIndex(language->findData("system"));
            QCOMPARE(QSettings().value("workspace/language").toString(), QString("system"));
            const QString expected = QLocale::system().language() == QLocale::Russian ? "ru" : "en";
            QCOMPARE(zerusUiLanguage(), expected);
        }
    }
};

QTEST_MAIN(TestUiLanguage)
#include "test_ui_language.moc"
