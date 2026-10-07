#include "IconFactory.h"

#include <QApplication>
#include <QDir>
#include <QPixmap>
#include <QTextStream>

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    const bool iconset = app.arguments().size() == 3 && app.arguments().at(1) == "--iconset";
    if (!iconset && app.arguments().size() != 2) {
        QTextStream(stderr) << "Usage: hgs-export-icons [--iconset] <output-directory>\n";
        return 2;
    }
    const QDir output(app.arguments().last());
    if (!output.exists()) {
        QTextStream(stderr) << "Output directory does not exist\n";
        return 2;
    }
    const auto save = [&](const QString &name, int size) {
        const QString path = output.filePath(name);
        if (!IconFactory::brandPixmap(size).save(path)) {
            QTextStream(stderr) << "Cannot write " << path << '\n';
            return false;
        }
        return true;
    };
    if (iconset) {
        for (int size : {16, 32, 128, 256, 512}) {
            if (!save(QStringLiteral("icon_%1x%1.png").arg(size), size)
                || !save(QStringLiteral("icon_%1x%1@2x.png").arg(size), size * 2)) return 1;
        }
    } else for (int size : {16, 24, 32, 48, 64, 128, 256, 512, 1024}) {
        if (!save(QStringLiteral("hgs-zerus-%1.png").arg(size), size)) return 1;
    }
    return 0;
}
