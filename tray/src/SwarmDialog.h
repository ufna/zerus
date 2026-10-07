#pragma once
#include <functional>
class HgsClient;
class SwarmController;
class FleetState;
class QWidget;
void showSwarmDialog(HgsClient *client, SwarmController *controller, const FleetState &fleet,
                     QWidget *parent, const std::function<void()> &manageConnections);
