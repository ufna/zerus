#pragma once

#include "TerminalBackend.h"

#include <QString>
#include <QStringList>

// Konsole по D-Bus. Раскладка проверена на Konsole 26.08 интроспекцией живых сервисов:
//   org.kde.konsole-<pid>            один сервис на процесс konsole
//   /Windows/<N>    org.kde.konsole.Window    newSession, sessionList, setCurrentSession
//   /Sessions/<N>   org.kde.konsole.Session   processId, runCommand, sendText, title
//   /konsole/MainWindow_<N>  org.qtproject.Qt.QWidget  raise/show/showNormal + свойства
// Индекс N у /Windows/<N> и /konsole/MainWindow_<N> общий (проверено: setTitle на
// сессии из /Windows/2 меняет windowTitle именно у MainWindow_2).
//
// Все D-Bus-вызовы синхронные с таймаутом 3с: они происходят по клику пользователя,
// а не в тике, и зависшая Konsole не должна вешать трей навсегда.
class KonsoleBackend : public TerminalBackend {
public:
    // auto  -- пробовать вкладку, тихо откатываясь на окно (сегодняшнее поведение).
    // tab   -- только вкладка; не вышло -- окно, но takeAdvisory() объяснит почему.
    // window -- всегда окно, open() вообще не трогает D-Bus (ни одного вызова).
    enum class OpenMode { Auto, Tab, Window };

    explicit KonsoleBackend(OpenMode mode = OpenMode::Auto);

    bool open(const QString &command, QString *error) override;
    bool canRaise() const override;
    bool raiseTty(const QString &tty, QString *error) override;
    QString takeAdvisory() override;

    // --- чистые части, проверяются тестом без десктопа и без D-Bus ---

    // tty процесса по /proc. Основной путь -- управляющий терминал из
    // /proc/<pid>/stat (поле tty_nr), запасной -- readlink /proc/<pid>/fd/0.
    // Пусто, если терминала нет или он не pts.
    static QString ttyOfPid(int pid);
    // Ядерная кодировка dev_t из /proc/<pid>/stat -> "/dev/pts/<N>". Не pts -> пусто.
    static QString ttyFromDevNumber(unsigned int devNumber);
    // "pts/4" и "/dev/pts/4" -- одно и то же; hgs отдаёт второе, ядро тоже, но
    // сравнивать всё равно надо в одном виде.
    static QString normalizeTty(const QString &tty);
    // Аргументы `konsole` для запуска новой команды отдельным окном.
    static QStringList launchArgs(const QString &command);

private:
    OpenMode m_mode;
    // "Уже объяснили в этом прогоне" -- tab-режим на выключенном API деградирует на
    // каждый клик одинаково; без этого флага пользователь получал бы один и тот же
    // балун на каждую сессию, которую открывает. Живёт на объекте (одна KonsoleBackend
    // на весь процесс трея), поэтому это ровно "один раз за прогон трея", не "один раз
    // за клик" и не "навсегда после первого запуска".
    bool m_tabAdvisoryShown = false;
    // Сообщение для takeAdvisory(), см. TerminalBackend.h. Выставляется только вместе
    // с успешным launchWindow() в open() -- не раньше, чтобы неудача самого запуска
    // окна не оставила противоречивую пару "не открылось, но вот пояснение как открылось".
    QString m_advisory;
};
