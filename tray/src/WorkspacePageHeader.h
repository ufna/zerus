#pragma once
#include "WorkspaceIcons.h"
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>

namespace WorkspacePageHeader {
inline QHBoxLayout *row(QLabel *title, QPushButton *refresh, QPushButton *create, QPushButton *secondary = nullptr)
{
    auto *layout = new QHBoxLayout;
    layout->setContentsMargins(0, 0, 0, 0); layout->setSpacing(12);
    title->setStyleSheet("font-size:16px; font-weight:600;");
    layout->addWidget(title, 1, Qt::AlignVCenter);
    layout->addWidget(refresh, 0, Qt::AlignVCenter);
    if (secondary) layout->addWidget(secondary, 0, Qt::AlignVCenter);
    if (create) layout->addWidget(create, 0, Qt::AlignVCenter);
    return layout;
}

inline void theme(QPushButton *refresh, QPushButton *create, const QString &refreshCaption, bool dark, QPushButton *secondary = nullptr)
{
    WorkspaceFocus::install();
    const QColor text(dark ? "#e8edf4" : "#1a2733");
    const QColor muted(dark ? "#a2adbc" : "#627082");
    const auto style = QStringLiteral(
        "QPushButton {font-size:13px;font-weight:400;background:%1;color:%2;border:1px solid %3;border-radius:7px;"
        "padding:7px 13px;min-height:20px;max-height:20px;}"
        "QPushButton:hover {background:%4;border-color:%5;}"
        "QPushButton:focus[keyboardFocus=\"true\"] {border-color:%5;}"
        "QPushButton:disabled {color:%6;}")
        .arg(dark ? "#1c2229" : "#ffffff", text.name(), dark ? "#333c47" : "#dce2e8",
             dark ? "#2b3540" : "#edf2f5", dark ? "#8bdfc0" : "#167357", muted.name());
    for (auto *button : {refresh, create, secondary}) if (button) {
        button->setFixedHeight(36); button->setIconSize(QSize(16, 16));
        button->setCursor(Qt::PointingHandCursor); button->setStyleSheet(style);
        button->setAccessibleName(button->text());
    }
    refresh->setText({}); refresh->setToolTip(refreshCaption); refresh->setAccessibleName(refreshCaption);
    refresh->setFixedSize(36, 36); refresh->setIconSize(QSize(18, 18));
    refresh->setStyleSheet(style + "QPushButton {padding:0;min-width:34px;max-width:34px;min-height:34px;max-height:34px;}");
    refresh->setIcon(workspaceIcon("refresh", muted));
    if (create) create->setIcon(workspaceIcon("add", text));
}
}
