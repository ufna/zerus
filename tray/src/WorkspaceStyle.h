#pragma once
#include <QString>
#include <QWidget>

// QWidget repolishes even an identical stylesheet, invalidating layout and
// repainting descendants. Live snapshots should only style changed values.
inline void setWorkspaceStyle(QWidget *widget, const QString &style)
{
    if (widget->styleSheet() != style) widget->setStyleSheet(style);
}

// Style every subcontrol: leaving page/arrow controls native lets some Linux
// styles paint a black groove even when the scrollbar itself is transparent.
inline QString workspaceScrollbars(bool dark)
{
    return QStringLiteral(R"(
        QScrollBar { background:transparent; border:0; }
        QScrollBar:vertical { width:7px; margin:2px 0; }
        QScrollBar:horizontal { height:7px; margin:0 2px; }
        QScrollBar::handle { background:%1; border:0; border-radius:3px; }
        QScrollBar::handle:vertical { min-height:28px; }
        QScrollBar::handle:horizontal { min-width:28px; }
        QScrollBar::handle:hover { background:%2; }
        QScrollBar::handle:pressed { background:%3; }
        QScrollBar::add-line, QScrollBar::sub-line { background:transparent; border:0; width:0; height:0; }
        QScrollBar::add-page, QScrollBar::sub-page { background:transparent; border:0; }
        QScrollBar::up-arrow, QScrollBar::down-arrow, QScrollBar::left-arrow, QScrollBar::right-arrow { background:transparent; border:0; width:0; height:0; }
        QAbstractScrollArea::corner { background:transparent; border:0; }
    )").arg(dark ? "rgba(156,170,183,95)" : "rgba(85,102,118,95)",
             dark ? "rgba(175,189,202,150)" : "rgba(85,102,118,145)",
             dark ? "rgba(190,205,218,190)" : "rgba(66,84,102,190)");
}
