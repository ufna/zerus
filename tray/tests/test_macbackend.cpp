#include <QtTest>
#include "MacBackend.h"

// Маковый бэкенд после удаления режима tabby (2026-08-28) -- это два построителя
// AppleScript и больше ничего: разбирать нечего (терминальный режим один), искать нечего
// (CLI Tabby на маке не существует -- см. врезку в MacBackend.h). Соответственно тестами
// покрыто ровно то, что осталось чистой функцией; open() по-прежнему не тестируется --
// он запускает osascript и открыл бы окно на рабочем столе пользователя.
class TestMacBackend : public QObject {
    Q_OBJECT
private slots:
    void terminalAppScriptQuotesCommand();
    void terminalAppScriptActivates();
    void notificationScriptEscapes();
};

void TestMacBackend::terminalAppScriptQuotesCommand()
{
    // В AppleScript строка в двойных кавычках: внутренние " и \ должны быть экранированы,
    // иначе имя сессии с кавычкой превратит скрипт в синтаксическую ошибку.
    const QString s = MacBackend::terminalAppScript(QStringLiteral("hgs a claude/a\"b"));
    QVERIFY(s.contains(QStringLiteral("do script")));
    QVERIFY(s.contains(QStringLiteral("claude/a\\\"b")));
}

void TestMacBackend::terminalAppScriptActivates()
{
    // Без activate окно открывается позади всех — проверено в TrustTunnelAgent.
    const QString s = MacBackend::terminalAppScript(QStringLiteral("true"));
    QVERIFY(s.contains(QStringLiteral("activate")));
}

// Текст уведомления -- это подстановка в литерал AppleScript, и в нём бывает и кавычка
// (имя сессии), и перенос строки (текст ошибки с командой). Сырой перенос внутри
// литерала -- синтаксическая ошибка: скрипт не выполнится и уведомление ПРОПАДЁТ, то
// есть ровно та беда, ради которой osascript сюда и позвали.
void TestMacBackend::notificationScriptEscapes()
{
    const QString s = MacBackend::notificationScript(
        QStringLiteral("hgs-tray: \"кавычка\""),
        QStringLiteral("строка\nвторая \\ слэш"));
    QVERIFY(s.startsWith(QStringLiteral("display notification ")));
    QVERIFY(s.contains(QStringLiteral("with title")));
    QVERIFY(s.contains(QStringLiteral("hgs-tray: \\\"кавычка\\\"")));
    // Ни одного сырого перевода строки в готовом скрипте.
    QVERIFY(!s.contains(QLatin1Char('\n')));
    QVERIFY(s.contains(QStringLiteral("строка\\nвторая")));
    QVERIFY(s.contains(QStringLiteral("\\\\ слэш")));
}

QTEST_APPLESS_MAIN(TestMacBackend)
#include "test_macbackend.moc"
