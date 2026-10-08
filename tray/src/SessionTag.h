#pragma once

#include <QObject>
#include <QString>

// The tag is the last part of a session name (agent/project/tag). Creating and
// renaming a session accept the same tags; `hgs rename` enforces the same rule
// (src/state/rename.rs).
namespace SessionTag {
inline QString problem(const QString &tag)
{
    if (tag.isEmpty()) return QObject::tr("Enter a session name.");
    if (tag != tag.trimmed()) return QObject::tr("Remove spaces at the beginning or end.");
    for (const QChar character : tag)
        if (character == '/' || character == ':' || character == '.' || character.category() == QChar::Other_Control)
            return QObject::tr("Names cannot contain /, : or . or control characters.");
    return {};
}
}
