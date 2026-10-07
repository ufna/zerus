#pragma once

#include "TerminalBackend.h"

#include <QString>
#include <QStringList>

// Запасной бэкенд: шаблон команды из ~/.config/hgs/tray.conf (`terminal=`) или из
// --terminal. Ничего не знает ни про Konsole, ни про D-Bus, поэтому годится и как
// «терминал, который выбрал пользователь», и как заглушка на платформе без бэкенда.
// Поднимать окно не умеет: узнать, где открыт tty, через запуск процесса нельзя.
class CommandBackend : public TerminalBackend {
public:
    explicit CommandBackend(QString terminalTemplate);

    bool open(const QString &command, QString *error) override;

    // Чистая функция — проверяется тестом без десктопа.
    //
    // Шаблон СНАЧАЛА режется на аргументы и только потом в каждом из них {cmd}
    // заменяется на команду. Порядок именно такой, чтобы команда всегда оставалась
    // ровно одним аргументом: имя сессии приходит из имени каталога проекта и может
    // содержать пробелы и кавычки, а склейка «подставить в строку, потом разрезать»
    // на таком имени разъедется (и это была бы дыра ровно того класса, что и
    // shell-инъекция). Плата за это -- шаблон обязан заканчиваться тем, что принимает
    // командную строку одним аргументом: `foot -e bash -lc {cmd}`, а не `xterm -e {cmd}`.
    static QStringList buildArgv(const QString &terminalTemplate, const QString &command,
                                 QString *error);

private:
    QString m_template;
};
