#pragma once
#include <QSettings>

namespace ProcessSettings {
inline bool enabled(){return QSettings().value("processes/enabled",false).toBool();}
inline int milliseconds(const char *key,double fallback,double minimum,double maximum) {
    return qRound(qBound(minimum,QSettings().value(QStringLiteral("processes/")+QLatin1String(key),fallback).toDouble(),maximum)*1000);
}
inline int activeMs(){return milliseconds("activeSeconds",2.5,1,60);}
inline int backgroundMs(){return milliseconds("backgroundSeconds",30,5,600);}
inline int unknownMs(){return milliseconds("unknownSeconds",10,1,600);}
inline int loadingMs(){return milliseconds("loadingSeconds",0.5,0,5);}
}
