#pragma once

#include <QString>

#include "TerminalBackend.h"

// macOS. Способ открыть сессию ровно один -- Terminal.app через osascript «do script»:
// работает всегда и без доустановок; ровно этим кодом TrustTunnelAgent открывает свои
// логи. ВСЕГДА новое окно: вкладку в существующем окне чистым AppleScript создать нельзя
// -- в словаре Terminal.app элемент tab у окна объявлен access="r".
//
// ПОЧЕМУ НЕ TABBY. Здесь был второй режим (`tabby run <argv>`, настоящая новая вкладка);
// он удалён 2026-08-28 вместе с tabbyArgs()/findTabby()/isAppBundleShim() -- искать их
// нужно в истории этого файла. Причина: команды `tabby` на маке взять НЕГДЕ, и это
// проверено на живой машине (Tabby 1.0.235, Electron 38, macOS 26.6.1), а не выведено из
// документации:
//   * бандл Tabby.app CLI не ставит вовсе («Shell integration» в настройках Tabby -- это
//     Automator-workflow'ы для Finder, а не команда);
//   * симлинк на Contents/MacOS/Tabby (то, что советовали и прошлые версии этой врезки, и
//     README) НЕ РАБОТАЕТ: запущенный по чужому пути Electron не находит свои helper-app
//     ("Unable to find helper app") и уходит в цикл падений GPU/network-процессов --
//     процесс живёт минутами и снимается только вручную;
//   * прямой запуск /Applications/Tabby.app/Contents/MacOS/Tabby с теми же argv поднимает
//     ВТОРОЙ экземпляр Tabby, чей разбор argv падает внутри yargs/cliui
//     ("mixin.stripAnsi is not a function") ДО того, как argv уедет в уже запущенный
//     экземпляр: вкладки нет, зато в Dock висит второй Tabby.
// Режим, который на этой машине не может сделать обещанного никогда, -- это пункт меню,
// который врёт; такой хуже, чем его отсутствие. Вместе с ним ушёл и весь механизм
// «объяснили один раз за прогон» (takeAdvisory/m_tabbyAdvisoryShown): объяснять теперь
// нечего -- Terminal.app делает ровно то, что обещает пункт меню. Вернуть режим имеет
// смысл только вместе с НАСТОЯЩЕЙ командой tabby (починенный апстрим, своя обёртка), и
// тогда же вернётся отбраковка симлинка в бандл.
//
// raise() не реализован: на маке у трея нет способа адресовать чужую вкладку, и клик по
// уже приаттаченной сессии откроет второй клиент на тот же экран. Это принято сознательно
// и записано в TerminalBackend.h.
class MacBackend : public TerminalBackend {
public:
    bool open(const QString &command, QString *error) override;

    // Чистые функции: только они и тестируются, запускать приложения в тестах нельзя.
    static QString terminalAppScript(const QString &command);

    // AppleScript «display notification». Живёт здесь, а не в TrayAgent, по двум
    // причинам: экранирование строки AppleScript уже написано и выверено в этом файле
    // (appleScriptEscape, порядок замен), а MacBackend.cpp — единственная единица
    // трансляции, которая собирается только на маке. Кто и зачем зовёт это вместо
    // QSystemTrayIcon::showMessage — см. TrayAgent::notify().
    static QString notificationScript(const QString &title, const QString &body);
};
