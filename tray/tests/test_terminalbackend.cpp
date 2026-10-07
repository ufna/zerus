#include <QtTest>
#include <QFile>
#include <QSet>
#include <QVector>
#include "ClipboardBackend.h"
#include "CommandBackend.h"
#include "SessionCommands.h"
#ifdef Q_OS_LINUX
#include "KonsoleBackend.h"
#include <unistd.h>
#endif

class TestTerminalBackend : public QObject {
    Q_OBJECT
private slots:
    void substitutesCommandAsOneArgument();
    void keepsCommandWithSpacesIntact();
    void rejectsEmptyTemplate();
    void rejectsTemplateWithoutPlaceholder();
    void substitutesInsideArgument();
    void sessionCommandsPreserveIdentityAndQuoting();
    void archiveCommandsRestoreTheSelectedInstance();
    void folderShellQuotesPathsAndUsesMachineProfiles();
    // ClipboardBackend -- только чистые части: цепочка способов, проверка команды и текст
    // уведомления. Сам буфер обмена тест не трогает (это буфер живого пользователя).
    void clipboardChainPrefersKlipperOnLinux();
    void clipboardChainOnMacIsQtOnly();
    void clipboardChainIsNeverEmpty();
    void clipboardMethodsHaveDistinctNames();
    void clipboardRejectsCommandWithNewline();
    void clipboardRejectsEmptyCommand();
    void clipboardAcceptsOrdinaryCommand();
    void clipboardNotificationKeepsShortCommandVerbatim();
    void clipboardNotificationElidesMiddleOfLongCommand();
#ifdef Q_OS_LINUX
    void decodesPtsDeviceNumbers();
    void ignoresNonPtsDevices();
    void ttyOfSelfMatchesProc();
    void ttyOfMissingProcessIsEmpty();
    void normalizesTty();
#endif
};

void TestTerminalBackend::sessionCommandsPreserveIdentityAndQuoting()
{
    QCOMPARE(SessionCommands::open({}, {}, "codex/docs/review"), QString("hgs a codex/docs/review"));
    QCOMPARE(SessionCommands::open("/opt/hgs", "mac", "codex/проект/Новый план"),
             QString("/opt/hgs @mac a 'codex/проект/Новый план'"));
    QCOMPARE(SessionCommands::open("/tmp/hgs's bin/hgs", "mac", "codex/docs/it's $(exit 7); `exit 8`"),
             QString("'/tmp/hgs'\\''s bin/hgs' @mac a 'codex/docs/it'\\''s $(exit 7); `exit 8`'"));
    QCOMPARE(SessionCommands::command({}, {"codex", "/work/a project", "--new", "-n", "plan"}),
             QString("hgs codex '/work/a project' --new -n plan"));
}

void TestTerminalBackend::folderShellQuotesPathsAndUsesMachineProfiles()
{
    QVERIFY(SessionCommands::folderShell({}, {}, "relative").isEmpty());
    QCOMPARE(SessionCommands::folderShell({}, {}, "/work/a folder"), QString("cd -- '/work/a folder' && exec \"${SHELL:-/bin/sh}\" -l"));
    QCOMPARE(SessionCommands::folderShell("/opt/hgs", "mac", "/work/it's $(literal)"),
        SessionCommands::command("/opt/hgs", {"machine","ssh","mac","--directory","/work/it's $(literal)"}));
}

void TestTerminalBackend::archiveCommandsRestoreTheSelectedInstance()
{
    QCOMPARE(SessionCommands::open("/usr/local/bin/hgs", "mac", "claude/infra/review", "archive-one"),
             QString("/usr/local/bin/hgs @mac resume claude/infra/review --archive archive-one"));
    QVERIFY(SessionCommands::open({}, {}, "codex/docs/review", "older") !=
            SessionCommands::open({}, {}, "codex/docs/review", "newer"));
    QCOMPARE(SessionCommands::open({}, {}, "codex/docs/review", "id with ' quotes"),
             QString("hgs resume codex/docs/review --archive 'id with '\\'' quotes'"));
}

void TestTerminalBackend::substitutesCommandAsOneArgument()
{
    QString err;
    const QStringList argv = CommandBackend::buildArgv(
        QStringLiteral("foot -e bash -lc {cmd}"), QStringLiteral("hgs a claude/x"), &err);
    QVERIFY2(err.isEmpty(), qPrintable(err));
    QCOMPARE(argv, QStringList({QStringLiteral("foot"), QStringLiteral("-e"),
                                QStringLiteral("bash"), QStringLiteral("-lc"),
                                QStringLiteral("hgs a claude/x")}));
}

void TestTerminalBackend::keepsCommandWithSpacesIntact()
{
    // Это и есть смысл «резать шаблон до подстановки»: команда с пробелами и кавычками
    // не должна разъехаться на несколько аргументов.
    QString err;
    const QStringList argv = CommandBackend::buildArgv(
        QStringLiteral("xterm -e sh -c \"{cmd}\""),
        QStringLiteral("hgs a 'claude/my project/tag'"), &err);
    QVERIFY2(err.isEmpty(), qPrintable(err));
    QCOMPARE(argv.size(), 5);
    QCOMPARE(argv.constLast(), QStringLiteral("hgs a 'claude/my project/tag'"));
}

void TestTerminalBackend::rejectsEmptyTemplate()
{
    QString err;
    QVERIFY(CommandBackend::buildArgv(QString(), QStringLiteral("hgs a x"), &err).isEmpty());
    QVERIFY(err.contains(QStringLiteral("{cmd}")));
}

void TestTerminalBackend::rejectsTemplateWithoutPlaceholder()
{
    QString err;
    QVERIFY(CommandBackend::buildArgv(QStringLiteral("konsole -e bash"),
                                      QStringLiteral("hgs a x"), &err).isEmpty());
    QVERIFY(err.contains(QStringLiteral("{cmd}")));
}

void TestTerminalBackend::substitutesInsideArgument()
{
    QString err;
    const QStringList argv = CommandBackend::buildArgv(
        QStringLiteral("alacritty --command={cmd}"), QStringLiteral("hgs a x"), &err);
    QVERIFY2(err.isEmpty(), qPrintable(err));
    QCOMPARE(argv.constLast(), QStringLiteral("--command=hgs a x"));
}

void TestTerminalBackend::clipboardChainPrefersKlipperOnLinux()
{
    // Порядок -- не косметика: klipper первым именно потому, что на Wayland он
    // единственный не упирается в требование serial'а от своего ввода (см. .h).
    const QVector<ClipboardBackend::Method> chain =
        ClipboardBackend::methodChain(ClipboardBackend::Platform::Linux);
    QCOMPARE(chain, QVector<ClipboardBackend::Method>({ClipboardBackend::Method::Klipper,
                                                       ClipboardBackend::Method::WlCopy,
                                                       ClipboardBackend::Method::QtClipboard}));
}

void TestTerminalBackend::clipboardChainOnMacIsQtOnly()
{
    // Смысл того, что платформа -- параметр, а не #ifdef: маковую цепочку видно и с Linux.
    const QVector<ClipboardBackend::Method> chain =
        ClipboardBackend::methodChain(ClipboardBackend::Platform::Mac);
    QCOMPARE(chain, QVector<ClipboardBackend::Method>({ClipboardBackend::Method::QtClipboard}));
}

void TestTerminalBackend::clipboardChainIsNeverEmpty()
{
    // Пустая цепочка означала бы open(), который не пробует ничего и отказывает молча.
    for (ClipboardBackend::Platform p :
         {ClipboardBackend::Platform::Linux, ClipboardBackend::Platform::Mac})
        QVERIFY(!ClipboardBackend::methodChain(p).isEmpty());
    QVERIFY(!ClipboardBackend::methodChain(ClipboardBackend::currentPlatform()).isEmpty());
}

void TestTerminalBackend::clipboardMethodsHaveDistinctNames()
{
    // Имена уходят в текст ошибки "не удалось (klipper: …; wl-copy: …)" -- одинаковые
    // или пустые сделали бы её бесполезной.
    QSet<QString> names;
    for (ClipboardBackend::Method m :
         {ClipboardBackend::Method::Klipper, ClipboardBackend::Method::WlCopy,
          ClipboardBackend::Method::QtClipboard}) {
        const QString n = ClipboardBackend::methodName(m);
        QVERIFY(!n.isEmpty());
        names.insert(n);
    }
    QCOMPARE(names.size(), 3);
}

void TestTerminalBackend::clipboardRejectsCommandWithNewline()
{
    // Главное правило режима: в буфер не попадает то, что оболочка выполнит сразу при
    // вставке. Отказ, а не тихая чистка -- иначе в буфере окажется не то, что показано
    // в уведомлении.
    QString err;
    QVERIFY(!ClipboardBackend::isCopyable(QStringLiteral("hgs a claude/x\nrm -rf /"), &err));
    QVERIFY2(!err.isEmpty(), "отказ обязан объяснить причину");
    err.clear();
    QVERIFY(!ClipboardBackend::isCopyable(QStringLiteral("hgs a claude/x\n"), &err));
    err.clear();
    QVERIFY(!ClipboardBackend::isCopyable(QStringLiteral("hgs a claude/x\r"), &err));
}

void TestTerminalBackend::clipboardRejectsEmptyCommand()
{
    QString err;
    QVERIFY(!ClipboardBackend::isCopyable(QString(), &err));
    QVERIFY(!err.isEmpty());
    QVERIFY(!ClipboardBackend::isCopyable(QStringLiteral("   "), &err));
}

void TestTerminalBackend::clipboardAcceptsOrdinaryCommand()
{
    QString err;
    QVERIFY2(ClipboardBackend::isCopyable(QStringLiteral("hgs @mac a claude/sample-project"), &err),
             qPrintable(err));
    // Пробелы и кавычки внутри имени -- обычное дело (имя проекта = имя каталога),
    // и они копированию не мешают: выполнять эту строку никто не будет.
    QVERIFY(ClipboardBackend::isCopyable(QStringLiteral("hgs a 'claude/my project/tag'"), &err));
}

void TestTerminalBackend::clipboardNotificationKeepsShortCommandVerbatim()
{
    // Всё, что трей строит в реальности, короче порога и должно доехать до балуна
    // дословно -- пользователь сверяет уведомление с тем, что вставит.
    const QString cmd = QStringLiteral("hgs @mac a claude/sample_web_application-front");
    QCOMPARE(ClipboardBackend::notificationText(cmd), cmd);
    QCOMPARE(ClipboardBackend::notificationText(QStringLiteral("hgs a claude/sample-project")),
             QStringLiteral("hgs a claude/sample-project"));
}

void TestTerminalBackend::clipboardNotificationElidesMiddleOfLongCommand()
{
    const QString cmd = QStringLiteral("hgs @somehost a claude/")
                        + QString(300, QLatin1Char('x')) + QStringLiteral("/tail-matters");
    const QString text = ClipboardBackend::notificationText(cmd);
    QVERIFY2(text.size() < cmd.size(), "длинная команда обязана сократиться");
    QVERIFY(text.size() <= 120);
    // Сокращение именно посередине: и голова (какой бокс), и хвост (какая сессия) целы.
    QVERIFY2(text.startsWith(QStringLiteral("hgs @somehost a")), qPrintable(text));
    QVERIFY2(text.endsWith(QStringLiteral("/tail-matters")), qPrintable(text));
    QVERIFY(text.contains(QStringLiteral("…")));
}

#ifdef Q_OS_LINUX
void TestTerminalBackend::decodesPtsDeviceNumbers()
{
    // Кодировка ядра: major 136 -- первый major под UNIX98 pts.
    QCOMPARE(KonsoleBackend::ttyFromDevNumber((136u << 8) | 4u), QStringLiteral("/dev/pts/4"));
    QCOMPARE(KonsoleBackend::ttyFromDevNumber((136u << 8) | 0u), QStringLiteral("/dev/pts/0"));
    // Второй major -- продолжение нумерации, а не отдельный ряд.
    QCOMPARE(KonsoleBackend::ttyFromDevNumber((137u << 8) | 1u), QStringLiteral("/dev/pts/257"));
    QCOMPARE(KonsoleBackend::ttyFromDevNumber((143u << 8) | 255u), QStringLiteral("/dev/pts/2047"));
}

void TestTerminalBackend::ignoresNonPtsDevices()
{
    QVERIFY(KonsoleBackend::ttyFromDevNumber(0).isEmpty());          // нет терминала
    QVERIFY(KonsoleBackend::ttyFromDevNumber((4u << 8) | 1u).isEmpty());   // /dev/tty1
    QVERIFY(KonsoleBackend::ttyFromDevNumber((5u << 8) | 0u).isEmpty());   // /dev/tty
    QVERIFY(KonsoleBackend::ttyFromDevNumber((144u << 8) | 0u).isEmpty()); // за пределами pts
}

void TestTerminalBackend::ttyOfSelfMatchesProc()
{
    // Под ctest у теста своего терминала нет, поэтому сравниваем не с константой, а с
    // тем, что показывает сам /proc: тест обязан работать и в консоли, и в CI.
    const QString expected = QFile::symLinkTarget(QStringLiteral("/proc/self/fd/0"));
    const QString actual = KonsoleBackend::ttyOfPid(static_cast<int>(getpid()));
    if (expected.startsWith(QStringLiteral("/dev/pts/")))
        QCOMPARE(actual, expected);
    else
        QVERIFY(actual.isEmpty());
}

void TestTerminalBackend::ttyOfMissingProcessIsEmpty()
{
    QVERIFY(KonsoleBackend::ttyOfPid(0).isEmpty());
}

void TestTerminalBackend::normalizesTty()
{
    QCOMPARE(KonsoleBackend::normalizeTty(QStringLiteral("pts/4")), QStringLiteral("/dev/pts/4"));
    QCOMPARE(KonsoleBackend::normalizeTty(QStringLiteral("/dev/pts/4")), QStringLiteral("/dev/pts/4"));
    QVERIFY(KonsoleBackend::normalizeTty(QStringLiteral("  ")).isEmpty());
}
#endif

QTEST_MAIN(TestTerminalBackend)
#include "test_terminalbackend.moc"
