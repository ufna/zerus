#pragma once
#include <QDateTime>
#include <QJsonObject>
#include <QLocale>
#include <QPushButton>
#include <QTimer>
#include <cmath>

namespace CacheStatus {
inline bool expired(const QJsonObject &data) {
    const auto cache=data.value("session_usage").toObject().value("prompt_cache").toObject();
    const auto deadline=cache.value("expires_at").toDouble();
    return data.value("cache_hint").toObject().value("status")=="cold"
        || (cache.value("status")=="warm"&&deadline>0&&deadline<=QDateTime::currentSecsSinceEpoch());
}
inline QString warning(const QJsonObject &data) {
    const auto hint=data.value("cache_hint").toObject();
    const auto tokens=hint.value("tokens").toDouble();
    if(hint.value("status")=="cold")return tokens>0?QObject::tr("The agent reports a cold cache (%1 tokens). Continuing will rebuild the cached context.").arg(QLocale().toString(qint64(tokens))):QObject::tr("The agent reports a cold cache. Continuing will rebuild the cached context.");
    if(expired(data))return QObject::tr("The estimated cache lifetime has ended. Continuing may rebuild the cached context.");
    if(hint.value("status")=="saving_hint")return QObject::tr("The agent suggests starting a new task with /clear to save %1 context tokens. The current cache may still be warm.").arg(QLocale().toString(qint64(tokens)));
    return {};
}
class Button:public QPushButton {
public:
    explicit Button(QWidget *parent=nullptr):QPushButton(parent){
        setObjectName("activityCache");setFixedHeight(24);setMinimumWidth(0);setMaximumWidth(170);
        setSizePolicy(QSizePolicy::Ignored,QSizePolicy::Fixed);setFocusPolicy(Qt::NoFocus);
        auto *timer=new QTimer(this);timer->setInterval(1000);connect(timer,&QTimer::timeout,this,[this]{if(isVisible())render();});timer->start();
    }
    void setData(const QJsonObject &value,bool recorded=false){data=value;m_recorded=recorded;render();}
    void setTheme(bool dark){m_dark=dark;render();}
private:
    void render(){
        const auto usage=data.value("session_usage").toObject();const auto cache=usage.value("prompt_cache").toObject();
        const auto remaining=cache.value("expires_at").toDouble()-QDateTime::currentSecsSinceEpoch();
        const bool cold=expired(data);QString text;QStringList detail;
        if(cold)text=tr("Cold cache");
        else if(remaining>0&&cache.value("status")=="warm")text=tr("Cache ~%1m").arg(qint64(std::ceil(remaining/60.)));
        if(cache.value("expires_at").toDouble()>0)detail<<tr("Estimated expiry from the TTL reported in native usage: %1.").arg(QDateTime::fromSecsSinceEpoch(qint64(cache.value("expires_at").toDouble())).toLocalTime().toString("d MMM HH:mm:ss"));
        else detail<<tr("This agent has not reported a cache expiry. No countdown is available.");
        if(cache.value("cache_read").isDouble())detail<<tr("Last request read %1 tokens from cache.").arg(QLocale().toString(cache.value("cache_read").toInteger()));
        const auto note=warning(data);if(!note.isEmpty())detail<<note;
        detail<<tr("Prompt, model or tool changes can invalidate the cache earlier. Starting a new conversation is always your choice.");
        if(m_recorded)detail<<tr("Last recorded session data.");
        setText(text);setToolTip(detail.join('\n'));setAccessibleName(text);setAccessibleDescription(toolTip());
        const auto color=cold?(m_dark?"#ff9ca8":"#b52d48"):remaining>0?(m_dark?"#72cdb2":"#237a62"):(m_dark?"#a1adbb":"#647386");
        setStyleSheet(QString("QPushButton{font-size:11px;text-align:right;padding:0 6px;min-height:0;background:transparent;border:0;color:%1;}").arg(color));
        setVisible(!text.isEmpty());
    }
    QJsonObject data;bool m_dark=true,m_recorded=false;
};
}
