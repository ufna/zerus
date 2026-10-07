#pragma once

#include <QRegularExpression>
#include <QString>
#include <QStringList>

// Terminal and clipboard actions must address the same CLI and session.
namespace SessionCommands {
inline QString shellQuote(const QString &value)
{
    static const QRegularExpression safe(QStringLiteral("^[A-Za-z0-9_@%+=:,./-]+$"));
    if (!value.isEmpty() && safe.match(value).hasMatch()) return value;
    QString escaped = value;
    escaped.replace(QLatin1String("'"), QLatin1String("'\\''"));
    return QLatin1Char('\'') + escaped + QLatin1Char('\'');
}

inline QString command(const QString &hgsPath, const QStringList &arguments)
{
    QString result = shellQuote(hgsPath.isEmpty() ? QStringLiteral("hgs") : hgsPath);
    for (const auto &argument : arguments) result += QLatin1Char(' ') + shellQuote(argument);
    return result;
}

inline QString open(const QString &hgsPath, const QString &host, const QString &name,
                    const QString &archiveId = {})
{
    QStringList arguments;
    if (!host.isEmpty()) arguments << QLatin1Char('@') + host;
    if (archiveId.isEmpty()) arguments << QStringLiteral("a") << name;
    else arguments << QStringLiteral("resume") << name << QStringLiteral("--archive") << archiveId;
    return command(hgsPath, arguments);
}

inline QString folderShell(const QString &hgsPath, const QString &host, const QString &directory)
{
    if (!directory.startsWith('/') || directory.contains(QChar::Null)) return {};
    const auto shell = QStringLiteral("cd -- ") + shellQuote(directory)
        + QStringLiteral(" && exec \"${SHELL:-/bin/sh}\" -l");
    return host.isEmpty() ? shell : command(hgsPath, {"machine", "ssh", host, "--directory", directory});
}
} // namespace SessionCommands
