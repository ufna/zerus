#pragma once
#include <QColor>
#include <QIcon>
#include <QPainter>
#include <QSettings>
#include <cmath>

namespace MachineAppearance {
inline QColor color(const QString &machine) {
    const QColor custom(QSettings().value("machines/colors/" + machine).toString());
    if (custom.isValid()) return custom;
    static const char *colors[]{"#5fbfa5", "#8ea9f4", "#c098e6", "#e7ae66", "#e38f9e", "#75bfcf", "#acc577"};
    quint32 hash = 2166136261u;
    for (const auto &byte : machine.toUtf8()) hash = (hash ^ quint8(byte)) * 16777619u;
    return QColor(colors[((hash >> 16) ^ hash) % 7]);
}
inline void setColor(const QString &machine, const QColor &value) {
    if (!machine.isEmpty() && value.isValid()) QSettings().setValue("machines/colors/" + machine, value.name());
}
inline void resetColor(const QString &machine) { QSettings().remove("machines/colors/" + machine); }
inline bool vivid(const QString &machine) { return QSettings().value("machines/vivid/" + machine, false).toBool(); }
inline void setVivid(const QString &machine, bool enabled) {
    if (!machine.isEmpty()) QSettings().setValue("machines/vivid/" + machine, enabled);
}
inline QColor blend(const QColor &a, const QColor &b, qreal amount) {
    return QColor::fromRgbF(a.redF() * (1-amount) + b.redF() * amount,
        a.greenF() * (1-amount) + b.greenF() * amount, a.blueF() * (1-amount) + b.blueF() * amount);
}
inline double luminance(const QColor &color) {
    const auto linear = [](double c) { return c <= .04045 ? c / 12.92 : std::pow((c + .055) / 1.055, 2.4); };
    return .2126 * linear(color.redF()) + .7152 * linear(color.greenF()) + .0722 * linear(color.blueF());
}
inline QColor textColor(const QColor &value, bool dark, bool bright = false) {
    if (!bright) return blend(value, dark ? QColor("#ffffff") : QColor("#111820"), dark ? .38 : .48);
    // Choose the higher WCAG contrast; vivid yellow and blue need different ink.
    return luminance(value) > .179 ? QColor("#000000") : QColor("#ffffff");
}
inline QColor backgroundColor(const QColor &value, bool dark, bool bright = false) {
    return bright ? value : blend(value, dark ? QColor("#171e25") : QColor("#ffffff"), dark ? .78 : .83);
}
inline QIcon icon(const QString &machine) {
    QIcon result;
    for (int size : {16, 24, 32, 48}) {
        QPixmap pixmap(size, size); pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap); painter.setRenderHint(QPainter::Antialiasing); painter.setPen(Qt::NoPen); painter.setBrush(color(machine));
        painter.drawEllipse(QRectF(size*.22, size*.22, size*.56, size*.56)); painter.end(); result.addPixmap(pixmap);
    }
    return result;
}
}
