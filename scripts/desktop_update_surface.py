"""Small anchor-checked desktop delta, applicable to old tree and current main."""

def replace_once(path,old,new):
    text=path.read_text()
    if text.count(old)!=1:raise ValueError('Desktop surface anchor changed: '+str(path))
    path.write_text(text.replace(old,new))

def apply(root):
    settings=root/'tray/src/SettingsPage.h'
    replace_once(settings,'    void openRecovery()', '    void openUpdates(){nav->setCurrentRow(4);}\n    void openRecovery()')
    header=root/'tray/src/SessionsWindow.h'
    replace_once(header,'    void showSessionList();','    void showSessionList();\n    void showUpdates();')
    window=root/'tray/src/SessionsWindow.cpp'
    replace_once(window,'#include "SettingsPage.h"','#include "SettingsPage.h"\n#include "UpdateController.h"')
    replace_once(window,'    m_connectionRetry = new QPushButton', '''    auto *updateEntry=new QPushButton(tr("Update available"));updateEntry->setObjectName("updatesAvailable");updateEntry->setFlat(true);statusLayout->addWidget(updateEntry);
    auto *updates=UpdateController::instance();auto updateEntryState=[updates,updateEntry]{updateEntry->setVisible(updates->updateAvailable());updateEntry->setToolTip(tr("Zerus %1 — open Updates").arg(updates->feed().version));};connect(updates,&UpdateController::changed,this,updateEntryState);connect(updateEntry,&QPushButton::clicked,this,&SessionsWindow::showUpdates);updateEntryState();
    m_connectionRetry = new QPushButton''')
    replace_once(window,'QPushButton#connectionRetry {','QPushButton#connectionRetry, QPushButton#updatesAvailable {')
    replace_once(window,'QWidget#settingsContent,','QWidget#settingsContent, QWidget#relaySettings,')
    replace_once(window,'void SessionsWindow::showWorkspaceSettings()','void SessionsWindow::showUpdates(){m_pages->setCurrentIndex(5);m_settingsPage->openUpdates();}\n\nvoid SessionsWindow::showWorkspaceSettings()')
    menu=root/'tray/src/TrayMenu.cpp'
    replace_once(menu,'#include "FleetState.h"','#include "FleetState.h"\n#include "UpdateController.h"')
    replace_once(menu,'    connect(dashboard, &QAction::triggered, this, &TrayMenu::sessionsRequested);', '''    connect(dashboard, &QAction::triggered, this, &TrayMenu::sessionsRequested);
    auto *updates=UpdateController::instance();if(updates->updateAvailable()){auto *entry=m_menu->addAction(tr("Update available: Zerus %1").arg(updates->feed().version));entry->setObjectName("updatesAvailable");connect(entry,&QAction::triggered,this,&TrayMenu::updatesRequested);}''')
    replace_once(root/'tray/src/TrayMenu.h','    void aboutToShow();','    void aboutToShow();\n    void updatesRequested();')
    agent=root/'tray/src/TrayAgent.cpp'
    replace_once(agent,'#include "SessionsWindow.h"','#include "SessionsWindow.h"\n#include "UpdateController.h"')
    replace_once(agent,'    m_icons.setForegroundOverride(m_cfg.iconColor);', '''    m_icons.setForegroundOverride(m_cfg.iconColor);
    auto *updates=UpdateController::instance();
    connect(updates,&UpdateController::changed,this,[this]{if(!m_menu.isOpen())m_menu.rebuild(m_state);});
    connect(updates,&UpdateController::newerReleaseVerified,this,[this](const QString &token,const QString &version){m_notifier->post(token,tr("Zerus update available"),tr("Zerus %1 is available. Open Updates for installation instructions.").arg(version));});
    connect(&m_menu,&TrayMenu::updatesRequested,this,[this]{showSessions();m_sessionsWindow->showUpdates();});''')
    replace_once(agent,'        const auto target = AttentionTracker::target(token);', '''        if(UpdateController::isUpdateToken(token)){
            auto *updates=UpdateController::instance();if(!updates->updateAvailable()||updates->updateToken()!=token)return;
            showSessions();m_sessionsWindow->showUpdates();
#ifdef Q_OS_LINUX
            if(!activationToken.isEmpty())KWindowSystem::setCurrentXdgActivationToken(activationToken);
            KWindowSystem::activateWindow(m_sessionsWindow->windowHandle());
#endif
            return;
        }
        const auto target = AttentionTracker::target(token);''')
    replace_once(agent,'        m_attention.deliveryFailed(token);','        if(UpdateController::isUpdateToken(token))return;\n        m_attention.deliveryFailed(token);')

    add_route_test(root)


def add_route_test(root):
    test=root/'tray/tests/test_sessionswindow.cpp'
    replace_once(test,'    void initTestCase();','    void initTestCase();\n    void updatesEntryOpensExactSettingsPage();')
    with test.open('a') as output:output.write('\nvoid TestSessionsWindow::updatesEntryOpensExactSettingsPage(){\n    SessionsWindow window(script());window.show();window.showUpdates();\n    auto *sections=window.findChild<QListWidget *>("settingsSections");QVERIFY(sections);QCOMPARE(sections->currentRow(),4);QCOMPARE(sections->currentItem()->text(),QString("Updates"));\n    QVERIFY(window.findChild<QWidget *>("settingsPage")->isVisible());window.showSessionList();window.showUpdates();QCOMPARE(sections->currentRow(),4);\n}\n')
