#pragma once
#include <QObject>
#include <memory>

// Native launcher/Dock badge, independent of the system tray and notification
// delivery. Zero hides the badge; the shell controls its appearance.
class ApplicationBadge : public QObject {
public:
    using QObject::QObject;
    virtual void setCount(int count) = 0;
};

std::unique_ptr<ApplicationBadge> makeApplicationBadge();
