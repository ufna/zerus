#pragma once
#include "ComposerToolbar.h"
#include <QDateTime>
#include <QJsonObject>
#include <QLocale>
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
struct ChipState { bool visible=false; ChipTone tone=ChipTone::Quiet; QString label,shortLabel; };
inline ChipState chipState(const QJsonObject &data) {
    const auto cache=data.value("session_usage").toObject().value("prompt_cache").toObject();
    const auto remaining=cache.value("expires_at").toDouble()-QDateTime::currentSecsSinceEpoch();
    if(expired(data))return {true,ChipTone::Danger,QObject::tr("Cold cache"),QObject::tr("Cold")};
    if(data.value("cache_hint").toObject().value("status")=="saving_hint")return {true,ChipTone::Warning,QObject::tr("Clear suggested"),QStringLiteral("/clear")};
    if(remaining>0&&cache.value("status")=="warm") {
        const auto minutes=qint64(std::ceil(remaining/60.));
        return {true,ChipTone::Success,QObject::tr("Cache ~%1m").arg(minutes),QObject::tr("~%1m").arg(minutes)};
    }
    return {};
}
// The former tooltip without the warning, which the popover shows on its own.
inline QString details(const QJsonObject &data,bool recorded) {
    const auto cache=data.value("session_usage").toObject().value("prompt_cache").toObject();QStringList detail;
    if(cache.value("expires_at").toDouble()>0)detail<<QObject::tr("Estimated expiry from the TTL reported in native usage: %1.").arg(QDateTime::fromSecsSinceEpoch(qint64(cache.value("expires_at").toDouble())).toLocalTime().toString("d MMM HH:mm:ss"));
    else detail<<QObject::tr("This agent has not reported a cache expiry. No countdown is available.");
    if(cache.value("cache_read").isDouble())detail<<QObject::tr("Last request read %1 tokens from cache.").arg(QLocale().toString(cache.value("cache_read").toInteger()));
    detail<<QObject::tr("Prompt, model or tool changes can invalidate the cache earlier. Starting a new conversation is always your choice.");
    if(recorded)detail<<QObject::tr("Last recorded session data.");
    return detail.join('\n');
}
// Ticks the warm-cache countdown once a second while it is shown.
class Chip:public ToolbarChip {
public:
    explicit Chip(QWidget *parent=nullptr):ToolbarChip(parent){
        setObjectName("cacheChip");
        auto *timer=new QTimer(this);timer->setInterval(1000);connect(timer,&QTimer::timeout,this,[this]{if(isActive())refresh();});timer->start();
    }
    void setData(const QJsonObject &value,bool recorded=false){m_data=value;m_recorded=recorded;refresh();}
private:
    void refresh(){
        const auto state=chipState(m_data);
        setLabels(state.label,state.shortLabel);setTone(state.tone);
        setIconName(state.tone==ChipTone::Success?QString():QStringLiteral("context-warning"));
        const auto note=warning(m_data);setDetail((note.isEmpty()?QString():note+'\n')+details(m_data,m_recorded));
        setActive(state.visible);
    }
    QJsonObject m_data;bool m_recorded=false;
};
}
