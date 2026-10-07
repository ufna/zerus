#pragma once
#include "MachineAppearance.h"
#include "WorkspaceIcons.h"
#include <QWidget>

namespace ProjectAppearance {
inline QColor fill(const QColor &color, bool dark, bool vivid) {
    return MachineAppearance::backgroundColor(color, dark, vivid);
}
inline QColor ink(const QColor &color, bool dark, bool vivid) {
    return MachineAppearance::textColor(color, dark, vivid);
}
inline void paint(QPainter *p, const QRect &rect, const QString &name, const QColor &color, bool dark, bool vivid, const QString &badge = {}) {
    p->save(); p->setRenderHint(QPainter::Antialiasing);
    const auto foreground=ink(color,dark,vivid);
    p->setPen(Qt::NoPen); p->setBrush(fill(color,dark,vivid)); p->drawRoundedRect(rect,6,6);
    workspaceIcon("projects",foreground).paint(p,QRect(rect.left()+10,rect.center().y()-8,16,16));
    auto font=p->font();font.setPixelSize(12);font.setWeight(QFont::DemiBold);p->setFont(font);p->setPen(foreground);
    int reserved=0;
    if(!badge.isEmpty()){
        auto badgeFont=font;badgeFont.setPixelSize(10);badgeFont.setWeight(QFont::Medium);p->setFont(badgeFont);
        reserved=QFontMetrics(badgeFont).horizontalAdvance(badge)+14;
        const QRect badgeRect(rect.right()-reserved-9,rect.center().y()-9,reserved,18);
        auto background=foreground;background.setAlpha(dark?32:22);p->setPen(Qt::NoPen);p->setBrush(background);p->drawRoundedRect(badgeRect,4,4);
        p->setPen(foreground);p->drawText(badgeRect,Qt::AlignCenter,badge);p->setFont(font);reserved+=7;
    }
    const auto text=rect.adjusted(34,0,-10-reserved,0);
    p->drawText(text,Qt::AlignVCenter|Qt::AlignLeft,QFontMetrics(font).elidedText(name,Qt::ElideRight,text.width()));p->restore();
}
}
class ProjectPreview : public QWidget {
public:
    explicit ProjectPreview(QWidget *parent=nullptr):QWidget(parent) {setFixedHeight(36);setMinimumWidth(180);setMaximumWidth(340);}
    void setProject(const QString &name, const QColor &color, bool vivid, bool isDefault=false) {m_name=name;m_color=color;m_vivid=vivid;m_default=isDefault;setAccessibleName(name+(isDefault?tr(", default project"):QString()));update();}
    void setTheme(bool dark) {m_dark=dark;update();}
protected:
    void paintEvent(QPaintEvent *) override {QPainter p(this);ProjectAppearance::paint(&p,rect().adjusted(0,2,0,-2),m_name,m_color,m_dark,m_vivid,m_default?tr("Default"):QString());}
private:
    QString m_name;QColor m_color;bool m_dark=false,m_vivid=false,m_default=false;
};
