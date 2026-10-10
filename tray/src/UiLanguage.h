#pragma once

#include <QCoreApplication>
#include <QLibraryInfo>
#include <QLocale>
#include <QSettings>
#include <QTranslator>

// Several lightweight widgets inherit tr() without declaring Q_OBJECT. The
// shared context keeps their translations available under the runtime context.
class ZerusTranslator : public QTranslator {
public:
    using QTranslator::QTranslator;
    QString translate(const char *context, const char *source,
                      const char *disambiguation = nullptr, int n = -1) const override {
        QString text = QTranslator::translate(context, source, disambiguation, n);
        if (text.isEmpty() && (!disambiguation || !*disambiguation))
            text = QTranslator::translate("Zerus", source, nullptr, n);
        return text;
    }
};

inline QString zerusUiLanguage() {
    const QString selected = QSettings().value("workspace/language", "system").toString();
    if (selected == "ru" || selected == "en") return selected;
    return QLocale::system().language() == QLocale::Russian ? "ru" : "en";
}

inline bool installZerusTranslation(QCoreApplication &app, ZerusTranslator &translator,
                                    QTranslator &qtTranslator) {
    if (zerusUiLanguage() != "ru") return true;
    if (!translator.load(":/hgs/translations/zerus_ru.qm")) return false;
    if (qtTranslator.load("qtbase_ru", QLibraryInfo::path(QLibraryInfo::TranslationsPath)))
        app.installTranslator(&qtTranslator);
    app.installTranslator(&translator);
    return true;
}
