#pragma once
#include <QFontDatabase>
#include <QStringList>

// First installed family of GitHub's monospace stack. The generic "monospace" is
// no macOS family: Qt spent ~60 ms populating aliases on its first use and then
// fell back to the proportional system font. Only the installed list is checked,
// which never triggers that alias lookup.
inline QString monospaceFamily()
{
    static const QString family = [] {
        const QStringList installed = QFontDatabase::families();
        for (const char *candidate : {"ui-monospace", "SFMono-Regular", "SF Mono", "Menlo", "Consolas", "Liberation Mono"})
            if (installed.contains(QLatin1String(candidate), Qt::CaseInsensitive)) return QString::fromLatin1(candidate);
        return QFontDatabase::systemFont(QFontDatabase::FixedFont).family();
    }();
    return family;
}
