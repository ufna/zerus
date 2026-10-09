#pragma once
#include <QDateTime>
#include <QFrame>
#include <QFontMetricsF>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QPainter>
#include <QProgressBar>
#include <QPushButton>
#include <QVBoxLayout>
#include <QLocale>
#include <cmath>
#include <QTimer>
#include "WorkspaceIcons.h"

namespace AccountUsage {
inline QString credits(const QString &value) {
    bool ok=false;const double number=value.toDouble(&ok);
    return ok && std::isfinite(number)?QLocale().toString(number,'f',2):QStringLiteral("—");
}
inline QString period(const QJsonObject &window) {
    const int minutes=window.value("window_minutes").toInt();
    const QString duration=minutes==10080 ? QStringLiteral("7d") : minutes && minutes%1440==0 ? QString::number(minutes/1440)+"d"
        : minutes && minutes%60==0 ? QString::number(minutes/60)+"h" : minutes ? QString::number(minutes)+"m" : QString();
    const QString name=window.value("label").toString();
    return name.isEmpty() ? (duration.isEmpty() ? QObject::tr("Usage") : duration) : name+(duration.isEmpty()?QString():" "+duration);
}
inline QDateTime reset(const QJsonValue &value) {
    if (value.isDouble()) return QDateTime::fromSecsSinceEpoch(qint64(value.toDouble()));
    return QDateTime::fromString(value.toString(),Qt::ISODate).toLocalTime();
}
inline bool current(const QJsonObject &w) { const auto at=reset(w.value("resets_at"));return !at.isValid() || at>QDateTime::currentDateTime(); }
inline QString remaining(const QDateTime &at) {
    if(!at.isValid())return QObject::tr("reset unknown");
    const qint64 minutes=qMax<qint64>(0,(QDateTime::currentDateTime().secsTo(at)+59)/60);
    if(!minutes)return QObject::tr("refresh needed");
    if(minutes>=1440)return QObject::tr("%1d %2h").arg(minutes/1440).arg((minutes%1440)/60);
    if(minutes>=60)return QObject::tr("%1h %2m").arg(minutes/60).arg(minutes%60);
    return QObject::tr("%1m").arg(minutes);
}
inline QString windowText(const QJsonObject &w) {
    if(!current(w))return period(w)+QObject::tr(" refresh");
    const double used=w.value("used_percent").toDouble();
    return period(w)+" "+QString::number(used,'f',0)+"%"+(used>=100?" ("+remaining(reset(w.value("resets_at")))+")":QString());
}
inline double highest(const QJsonObject &data) {
    double used=-1;for(const auto &value:data.value("windows").toArray()){const auto w=value.toObject();if(current(w)&&w.value("used_percent").isDouble())used=qMax(used,w.value("used_percent").toDouble());}return used;
}
inline QString exhausted(const QJsonObject &data) {
    QStringList lines;
    for(const auto &value:data.value("windows").toArray()){
        const auto w=value.toObject();if(w.value("used_percent").toDouble()<100)continue;const auto at=reset(w.value("resets_at"));
        lines<<(!current(w)?QObject::tr("%1 limit window ended; refresh usage").arg(period(w)):
            at.isValid()?QObject::tr("%1 limit reached. Resets in %2").arg(period(w),remaining(at)):QObject::tr("%1 limit reached. Reset time is unavailable").arg(period(w)));
    }
    const auto text=lines.join("\n");
    return !text.isEmpty() && (data.value("offline").toBool() || data.value("refresh_error").toBool())
        ? QObject::tr("Last reported usage: %1. Refresh to check current limits.").arg(text) : text;
}
// Chip label: the nearest reset of an exhausted current window, or a reminder to
// refresh when only windows that already ended still report 100%.
inline QString exhaustedSummary(const QJsonObject &data) {
    QDateTime nearest;bool reached=false,ended=false;
    for(const auto &value:data.value("windows").toArray()){
        const auto w=value.toObject();if(w.value("used_percent").toDouble()<100)continue;
        if(!current(w)){ended=true;continue;}
        reached=true;const auto at=reset(w.value("resets_at"));
        if(at.isValid()&&(!nearest.isValid()||at<nearest))nearest=at;
    }
    if(reached)return nearest.isValid()?QObject::tr("Limit reached · %1").arg(remaining(nearest)):QObject::tr("Limit reached");
    return ended?QObject::tr("Refresh usage"):QString();
}
inline QColor color(double value,bool dark) {return QColor(value>=90 ? (dark?"#f07878":"#ce3d47") : value>=70 ? (dark?"#f0a35b":"#bd6519") : (dark?"#72cdb2":"#237a62"));}
inline QString state(const QJsonObject &data) {
    return data.isEmpty() || data.value("status")=="loading" ? QObject::tr("Loading account details…")
        : data.value("status")=="configured" ? QObject::tr("API key configured. Usage is available in the DeepSeek console.")
        : data.value("offline").toBool() || data.value("status")=="offline" ? QObject::tr("Machine is offline. Showing the last available account data.") : data.value("status")=="expired" ? QObject::tr("Native sign-in has expired. Open the agent or sign in again, then refresh usage.")
        : data.value("status")=="signed_out" && data.value("provider")=="dsh" ? QObject::tr("DeepSeek account is signed out. API-key connections are configured separately in the native settings.")
        : data.value("status")=="signed_out" ? QObject::tr("No native subscription sign-in found. API-key sessions may still work.")
        : data.value("status")=="credentials_locked" ? QObject::tr("Saved sign-in is in the locked macOS Keychain. Unlock it in Keychain Access, then refresh.")
        : data.value("status")=="desktop_session_unavailable" ? QObject::tr("Saved sign-in requires the Mac desktop session. Log in to macOS and check the HGS installation, then refresh.")
        : data.value("status")=="credentials_unavailable" ? QObject::tr("The saved Keychain sign-in could not be used. Check account access or sign in again.")
        : QObject::tr("Usage is unavailable from this provider. Open its native account page or refresh after signing in.");
}
inline QString tooltip(const QJsonObject &data) {
    QStringList lines{data.value("label").toString()};
    const auto identity=data.value("identity").toObject();
    for(const auto *key:{"name","email","plan","organization"}) if(!identity.value(key).toString().isEmpty())lines<<identity.value(key).toString();
    for(const auto &value:data.value("windows").toArray()) {
        const auto w=value.toObject();const auto at=reset(w.value("resets_at"));
        lines<<QObject::tr("%1: %2% used").arg(period(w)).arg(w.value("used_percent").toDouble(),0,'f',0);
        if(at.isValid())lines<<QObject::tr("Resets %1").arg(at.toString("d MMM HH:mm"));
    }
    for(const auto &value:data.value("balances").toArray()) {const auto b=value.toObject();lines<<b.value("kind").toString()+": "+b.value("balance").toString()+" "+b.value("currency").toString();}
    if(data.value("checked_at").isDouble())lines<<QObject::tr("Updated %1").arg(QDateTime::fromSecsSinceEpoch(qint64(data.value("checked_at").toDouble())).toString("HH:mm"));
    if(data.value("refreshing").toBool())lines<<QObject::tr("Updating… Previous values remain visible.");
    else if(data.value("refresh_error").toBool())lines<<QObject::tr("Update failed. Showing the last available values.");
    if(data.value("offline").toBool())lines<<QObject::tr("Machine is offline.");
    if(data.value("windows").toArray().isEmpty() && data.value("balances").toArray().isEmpty())lines<<state(data);
    lines<<QObject::tr("Updates every 5 minutes. Open account details.");
    return lines.join('\n');
}
class RefreshButton : public QPushButton {
public:
    explicit RefreshButton(QWidget *parent=nullptr):QPushButton(parent) {
        setObjectName("sessionUsageRefresh");setFixedSize(22,22);setIconSize(QSize(14,14));setCursor(Qt::PointingHandCursor);
        setAccessibleName(tr("Refresh account usage"));setToolTip(tr("Refresh account usage (automatically every 5 minutes)"));m_timer.setInterval(40);
        connect(&m_timer,&QTimer::timeout,this,[this]{m_angle=(m_angle+18)%360;update();});setTheme(true);
    }
    void setTheme(bool dark) {
        m_dark=dark;m_icon=workspaceIcon("refresh",QColor(dark?"#b8c8d6":"#506278"));m_busyIcon=workspaceIcon("refresh",QColor(dark?"#8ce3c9":"#237a62"));if(!m_refreshing)setIcon(m_icon);
        setStyleSheet(QString("QPushButton{padding:0;min-height:20px;min-width:20px;border:1px solid transparent;border-radius:4px;background:transparent;} QPushButton:hover{background:%1;border-color:%2;} QPushButton:pressed{background:%3;} QPushButton:focus[keyboardFocus=true]{border-color:%2;}").arg(dark?"#34434d":"#dde8ef",dark?"#799da5":"#64868f",dark?"#49616b":"#bfd3df"));update();
    }
    void setRefreshing(bool refreshing) {
        if(m_refreshing==refreshing)return;m_refreshing=refreshing;setProperty("refreshing",refreshing);m_angle=0;
        if(refreshing){setIcon({});m_timer.start();}else{m_timer.stop();setIcon(m_icon);}update();
        setToolTip(refreshing?tr("Updating account usage…"):tr("Refresh account usage (automatically every 5 minutes)"));
    }
    bool isRefreshing() const {return m_refreshing;}
protected:
    void paintEvent(QPaintEvent *event) override {
        QPushButton::paintEvent(event);if(!m_refreshing)return;
        QPainter p(this);p.setRenderHint(QPainter::SmoothPixmapTransform);p.translate(width()/2.,height()/2.);p.rotate(m_angle);
        p.drawPixmap(QRect(-8,-8,16,16),m_busyIcon.pixmap(32,32));
    }
private:QTimer m_timer;QIcon m_icon,m_busyIcon;bool m_dark=true,m_refreshing=false;int m_angle=0;
};
class Button : public QPushButton {
public:
    explicit Button(QWidget *parent=nullptr):QPushButton(parent) {
        setObjectName("sessionAccountUsage");setFixedHeight(22);setSizePolicy(QSizePolicy::Fixed,QSizePolicy::Fixed);
        setStyleSheet("QPushButton{padding:0;min-height:20px;border:0;background:transparent;text-align:right;}");
        setFixedWidth(contentWidth());
        auto *timer=new QTimer(this);timer->setInterval(30000);connect(timer,&QTimer::timeout,this,[this]{if(!isVisible())return;setFixedWidth(contentWidth());setToolTip(tooltip(m_data));setAccessibleName(tr("Account usage")+"\n"+exhausted(m_data)+"\n"+toolTip());update();});timer->start();
    }
    void setData(const QJsonObject &data) {if(m_data==data)return;m_data=data;setFixedWidth(contentWidth());setToolTip(tooltip(data));setAccessibleName(QObject::tr("Account usage")+"\n"+exhausted(data)+"\n"+toolTip());update();}
    void setTheme(bool dark) {m_dark=dark;update();}
    void setShowProvider(bool show) {m_showProvider=show;setFixedWidth(contentWidth());update();}
    void setCompact(bool compact) {m_compact=compact;setFixedWidth(contentWidth());update();}
    QSize sizeHint() const override {return {contentWidth(),22};}
protected:
    void changeEvent(QEvent *event) override {
        QPushButton::changeEvent(event);
        if(event->type()==QEvent::FontChange || event->type()==QEvent::StyleChange)setFixedWidth(contentWidth());
    }
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);p.setRenderHint(QPainter::Antialiasing);p.setPen(Qt::NoPen);
        if(underMouse() || hasFocus()){p.setBrush(QColor(m_dark?"#27313b":"#e8edf3"));p.drawRoundedRect(rect(),5,5);}
        QFont f=font();f.setPixelSize(11);p.setFont(f);const auto muted=QColor(m_dark?"#a1adbb":"#647386");
        const auto windows=m_data.value("windows").toArray();const bool loading=m_data.isEmpty() || m_data.value("status")=="loading";
        const int slotCount=loading?2:qMin(2,int(windows.size()));
        const int provider=providerWidth();
        int x=m_compact?4:6;
        if(provider){p.setPen(muted);p.drawText(QRect(x,0,provider-12,height()),Qt::AlignVCenter,m_data.value("provider").toString());x+=provider;}
        for(int i=0;i<slotCount;++i) {
            const auto w=loading?QJsonObject():windows[i].toObject();const double used=w.value("used_percent").toDouble();const bool fresh=!loading && current(w);
            const int barWidth=m_compact?20:34, textOffset=barWidth+6;
            const QString text=windowText(w);
            const int textWidth=indicatorTextWidth(w,loading);
            p.setPen(Qt::NoPen);p.setBrush(QColor(m_dark?"#36434e":"#d7e0e6"));p.drawRoundedRect(QRectF(x,9,barWidth,4),2,2);
            if(fresh){p.setBrush(color(used,m_dark));p.drawRoundedRect(QRectF(x,9,barWidth*qBound(0.,used,100.)/100.,4),2,2);}
            if(loading){p.setBrush(QColor(m_dark?"#2b3740":"#e3e8ed"));p.drawRoundedRect(QRectF(x+textOffset,7,textWidth,8),3,3);}
            else {p.setPen(fresh?color(used,m_dark):muted);p.drawText(QRect(x+textOffset,0,textWidth,height()),Qt::AlignVCenter,p.fontMetrics().elidedText(text,Qt::ElideRight,textWidth));}
            x+=textOffset+textWidth+(m_compact?10:16);
        }
        if(!slotCount) {
            const int available=width()-x-(m_compact?4:6);
            p.setPen(muted);p.drawText(QRect(x,0,available,height()),Qt::AlignVCenter|Qt::AlignRight,p.fontMetrics().elidedText(summaryText(),Qt::ElideRight,available));
        }
    }
private:
    int compactTextWidth(const QString &text) const {
        QFont f=font();f.setPixelSize(11);
        // Fractional glyph advances can exceed the rounded integer width.
        // Leave room for the complete percentage, including its last glyph.
        return qMin(150,int(std::ceil(QFontMetricsF(f).horizontalAdvance(text)))+2);
    }
    int providerWidth() const {const auto name=m_data.value("provider").toString();return m_showProvider&&!m_compact&&!name.isEmpty()?compactTextWidth(name)+12:0;}
    int indicatorTextWidth(const QJsonObject &window,bool loading) const {
        if(loading)return m_compact?40:48;
        const int text=compactTextWidth(windowText(window));
        // Percent changes keep a small stable slot; only additional text, such
        // as a reset countdown, expands it.
        return m_compact?text:qMax(text,compactTextWidth(period(window)+" 100%"));
    }
    QString summaryText() const {
        const auto balances=m_data.value("balances").toArray();const auto b=balances.isEmpty()?QJsonObject():balances.first().toObject();
        return !b.isEmpty()?b.value("balance").toString()+" "+b.value("currency").toString():m_data.value("status")=="configured"?tr("API key"):m_data.value("status")=="signed_out"?tr("Sign in required"):m_data.value("status")=="expired"?tr("Sign-in expired"):tr("Unavailable");
    }
    int contentWidth() const {
        const auto windows=m_data.value("windows").toArray();const bool loading=m_data.isEmpty() || m_data.value("status")=="loading";
        int width=(m_compact?8:12)+providerWidth();
        if(!loading&&windows.isEmpty())return width+compactTextWidth(summaryText());
        for(int i=0;i<(loading?2:qMin(2,int(windows.size())));++i)
            width+=(m_compact?26:40)+indicatorTextWidth(loading?QJsonObject():windows[i].toObject(),loading)+(i?(m_compact?10:16):0);
        return width;
    }
    QJsonObject m_data;bool m_dark=true,m_showProvider=true,m_compact=false;
};
class Panel : public QFrame {
public:
    explicit Panel(QWidget *parent=nullptr):QFrame(parent) {setObjectName("accountUsagePanel");m_layout=new QVBoxLayout(this);m_layout->setContentsMargins(14,12,14,12);m_layout->setSpacing(8);setMaximumWidth(620);}
    void setTheme(bool dark){m_dark=dark;setStyleSheet(QString("QFrame#accountUsagePanel{background:%1;border:1px solid %2;border-radius:9px;} QLabel{background:transparent;border:0;color:%3;} QProgressBar{border:0;border-radius:2px;background:%2;}").arg(dark?"#1d252c":"#ffffff",dark?"#33414c":"#d7e0e6",dark?"#a1adbb":"#647386"));const auto data=m_data;m_data={};setData(data);}
    void setData(const QJsonObject &data) {
        if(m_data==data && m_layout->count())return;m_data=data;while(auto *item=m_layout->takeAt(0)){delete item->widget();delete item;}
        auto addText=[this](const QString &text){auto *label=new QLabel(text);label->setTextFormat(Qt::PlainText);label->setWordWrap(true);m_layout->addWidget(label);};
        addText(QObject::tr("Account usage"));addText(QObject::tr("Shared by sessions using this account."));const auto windows=data.value("windows").toArray();const auto balances=data.value("balances").toArray();
        if(windows.isEmpty() && balances.isEmpty())addText(state(data));
        for(const auto &value:windows){const auto w=value.toObject();const double used=w.value("used_percent").toDouble();const bool fresh=current(w);const auto at=reset(w.value("resets_at"));
            addText(period(w)+"   "+(fresh?QObject::tr("%1% used").arg(used,0,'f',0):QObject::tr("Window ended; refresh for current usage")));
            auto *bar=new QProgressBar;bar->setRange(0,1000);bar->setValue(fresh?qRound(qBound(0.,used,100.)*10):0);bar->setTextVisible(false);bar->setFixedHeight(5);bar->setStyleSheet(QString("QProgressBar::chunk{background:%1;border-radius:2px;}").arg(color(used,m_dark).name()));m_layout->addWidget(bar);
            if(at.isValid())addText(QObject::tr("Resets %1").arg(at.toString("d MMM HH:mm")));
        }
        for(const auto &value:balances){const auto b=value.toObject();addText(b.value("kind").toString()+": "+b.value("balance").toString()+" "+b.value("currency").toString());}
        const auto credits=data.value("credits").toObject();
        if(!credits.isEmpty())addText(credits.value("unlimited").toBool()?QObject::tr("Credits: unlimited"):QObject::tr("Credits available: %1").arg(AccountUsage::credits(credits.value("balance").toString())));
        if(data.value("checked_at").isDouble())addText(QObject::tr("Updated %1").arg(QDateTime::fromSecsSinceEpoch(qint64(data.value("checked_at").toDouble())).toString("HH:mm")));
    }
private:QVBoxLayout *m_layout;QJsonObject m_data;bool m_dark=true;
};
}
