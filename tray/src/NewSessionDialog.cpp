#include "NewSessionDialog.h"
#include "WorkspaceIcons.h"
#include "DirectoryDialog.h"
#include "WorktreePanel.h"
#include "NewWorktreeDialog.h"
#include <QCheckBox>
#include <QToolButton>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFormLayout>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRegularExpression>
#include <QSettings>
#include <QSignalBlocker>
#include <QUuid>
#include <QVBoxLayout>
namespace {
QLabel *plainLabel(const QString &text = {}) {auto *l=new QLabel(text);l->setTextFormat(Qt::PlainText);l->setWordWrap(true);return l;}
}
NewSessionDialog::NewSessionDialog(const QString &hgsPath,const FleetState &fleet,const QString &selectedHost,
    const QString &agent,const QString &project,QWidget *parent)
    : QDialog(parent),m_client(hgsPath,this),m_fleet(fleet),m_initialProject(project),m_initialHost(selectedHost)
{
    setObjectName("newSessionDialog");setWindowTitle(tr("New session"));resize(590,540);setMinimumWidth(520);
    auto *layout=new QVBoxLayout(this);layout->setContentsMargins(28,24,28,24);layout->setSpacing(18);
    auto *heading=plainLabel(tr("New session"));heading->setObjectName("heading");layout->addWidget(heading);
    auto *form=new QFormLayout;form->setSpacing(12);form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    m_project=new QComboBox;m_project->setObjectName("launchProject");m_project->setAccessibleName(tr("Project"));
    m_manage=new QToolButton;m_manage->setText(tr("Folders…"));m_manage->setObjectName("manageLaunchProject");
    m_manage->setAutoRaise(true);m_manage->setToolTip(tr("Manage this project's folders"));
    auto *projectRow=new QHBoxLayout;projectRow->setSpacing(8);projectRow->addWidget(m_project,1);projectRow->addWidget(m_manage);
    form->addRow(tr("Project"),projectRow);
    m_machine=new QComboBox;m_machine->setObjectName("launchComputer");m_machine->setAccessibleName(tr("Computer"));form->addRow(tr("Computer"),m_machine);
    m_folder=new QComboBox;m_folder->setObjectName("launchProjectFolder");m_folder->setAccessibleName(tr("Working folder"));
    m_folder->setMinimumContentsLength(16);m_folder->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
    m_folder->setPlaceholderText(tr("Choose a folder…"));
    m_browse=new QPushButton(tr("Browse…"));m_browse->setObjectName("browseLaunchFolder");m_browse->setAutoDefault(false);
    auto *folderRow=new QHBoxLayout;folderRow->setSpacing(8);folderRow->addWidget(m_folder,1);folderRow->addWidget(m_browse);
    auto *folderField=new QVBoxLayout;folderField->setSpacing(6);folderField->addLayout(folderRow);
    m_projectPath=plainLabel();m_projectPath->setObjectName("launchFolderPath");m_projectPath->setTextInteractionFlags(Qt::TextSelectableByMouse);
    m_projectPath->setWordWrap(false);
    m_projectPath->setSizePolicy(QSizePolicy::Ignored,QSizePolicy::Preferred);folderField->addWidget(m_projectPath);
    m_preview=plainLabel();m_preview->setObjectName("muted");folderField->addWidget(m_preview);
    auto *worktreeActions=new QHBoxLayout;worktreeActions->setSpacing(8);
    m_worktrees=new QPushButton(tr("Other worktrees…"));m_worktrees->setObjectName("chooseLaunchWorktree");m_worktrees->setAutoDefault(false);m_worktrees->hide();worktreeActions->addWidget(m_worktrees);
    m_newWorktree=new QPushButton(tr("New worktree…"));m_newWorktree->setObjectName("newLaunchWorktree");m_newWorktree->setAutoDefault(false);m_newWorktree->hide();worktreeActions->addWidget(m_newWorktree);
    // Keep the Agent/Account rows in place while folder details and asynchronous
    // worktree discovery change. Hidden actions still reserve their row.
    for(auto *widget:QList<QWidget *>{m_projectPath,m_worktrees,m_newWorktree}) {
        auto policy=widget->sizePolicy();policy.setRetainSizeWhenHidden(true);widget->setSizePolicy(policy);
    }
    worktreeActions->addStretch();folderField->addLayout(worktreeActions);
    connect(m_worktrees,&QPushButton::clicked,this,&NewSessionDialog::chooseWorktree);
    connect(m_newWorktree,&QPushButton::clicked,this,&NewSessionDialog::createWorktree);
    form->addRow(tr("Working folder"),folderField);
    m_agent=new QComboBox;m_agent->setObjectName("launchAgent");m_agent->addItems({"codex","claude","kimi","dsh","sh"});
    if(!agent.isEmpty()&&m_agent->findText(agent)<0)m_agent->addItem(agent);
    if(!agent.isEmpty())m_agent->setCurrentText(agent);
    m_account=new QComboBox;m_account->setObjectName("launchAccount");m_account->addItem(tr("Default account"),QString());
    form->addRow(tr("Agent"),m_agent);form->addRow(tr("Account"),m_account);
    m_name=new QLineEdit("work-"+QUuid::createUuid().toString(QUuid::Id128).left(8));m_name->setObjectName("launchName");form->addRow(tr("Session name"),m_name);
    m_openTerminal=new QCheckBox(tr("Open terminal window"));m_openTerminal->setObjectName("launchOpenTerminal");
    m_openTerminal->setChecked(QSettings().value("workspace/launchOpenTerminal",false).toBool());form->addRow(QString(),m_openTerminal);
    layout->addLayout(form);
    m_error=plainLabel();m_error->setObjectName("launchError");layout->addWidget(m_error);
    layout->addStretch();auto *buttons=new QDialogButtonBox(QDialogButtonBox::Cancel);m_start=buttons->addButton(tr("Start session"),QDialogButtonBox::AcceptRole);m_start->setObjectName("primary");layout->addWidget(buttons);
    connect(buttons,&QDialogButtonBox::rejected,this,&QDialog::reject);connect(m_start,&QPushButton::clicked,this,&NewSessionDialog::start);
    connect(m_manage,&QToolButton::clicked,this,[this]{const auto id=m_project->currentData().toString();reject();emit projectFoldersRequested(id);});
    connect(m_project,&QComboBox::currentIndexChanged,this,[this]{loadMachines();loadFolders();});
    connect(m_machine,&QComboBox::currentIndexChanged,this,[this]{m_accountPreset=false;loadFolders();});
    connect(m_browse,&QPushButton::clicked,this,&NewSessionDialog::browseFolder);
    connect(m_folder,&QComboBox::currentIndexChanged,this,[this]{
        m_browsedFolders.remove(host());
        loadAccounts();updateForm();updateWorktrees();
    });
    connect(m_agent,&QComboBox::currentIndexChanged,this,[this]{m_accountPreset=false;updateAccounts();updateForm();});
    connect(m_account,&QComboBox::activated,this,[this]{m_preferredAccount=m_account->currentData().toString();m_accountPreset=true;updateForm();});
    connect(m_name,&QLineEdit::textChanged,this,&NewSessionDialog::updateForm);
    connect(&m_client,&HgsClient::directoriesReady,this,[this](quint64 request,const QString &machine,const QJsonObject &data){
        if(!m_validation||request!=m_validation||machine!=host())return;
        m_validation=0;
        const auto path=data.value("path").toString();
        if(!path.startsWith('/')) {m_error->setText(tr("The computer did not confirm this folder. Try again."));updateForm();return;}
        launch(path);
    });
    connect(&m_client,&HgsClient::directoriesFailed,this,[this](quint64 request,const QString &machine,const QString &error){
        if(!m_validation||request!=m_validation||machine!=host())return;
        m_validation=0;m_error->setText(error);updateForm();
    });
    connect(&m_client,&HgsClient::accountsReady,this,[this](quint64 id,const QString &machine,const QJsonObject &data){
        if(id!=m_accountRequest||machine!=host())return;
        m_accountRequest=0;m_accounts=data["profiles"].toArray();m_removedAccounts=data["removed_profiles"].toArray();updateAccounts();
    });
    connect(&m_client,&HgsClient::accountsFailed,this,[this](quint64 id,const QString &machine,const QString &error){
        if(id!=m_accountRequest||machine!=host())return;
        m_accountRequest=0;m_accounts={};updateAccounts();m_account->setToolTip(tr("Could not load accounts: %1").arg(error));
    });
    connect(&m_client,&HgsClient::worktreesReady,this,[this](quint64 request,const QString &machine,const QString &path,const QJsonObject &data){
        if(request!=m_catalogRequest||machine!=host()||path!=m_folder->currentData(Qt::UserRole+2).toString())return;
        m_catalogRequest=0;m_worktreeCatalog=data;m_worktrees->setVisible(!data["worktrees"].toArray().isEmpty());
        m_newWorktree->setVisible(data["state"]=="ok");updateForm();
    });
    connect(&m_client,&HgsClient::worktreesFailed,this,[this](quint64 request,const QString &,const QString &,const QString &){
        if(request!=m_catalogRequest)return;m_catalogRequest=0;m_worktreeCatalog["stale"]=true;updateForm();
    });
    setGroups(SessionOrganization(QJsonDocument::fromJson(QSettings().value("workspace/organization").toByteArray()).object()),project);
}
QString NewSessionDialog::host() const {return m_machine->currentData().toString();}
void NewSessionDialog::setFleet(const FleetState &fleet)
{
    m_fleet=fleet;
    loadMachines();loadFolders();
}
void NewSessionDialog::loadMachines()
{
    const auto previous=m_machine->currentIndex()<0?m_initialHost:host();
    const QSignalBlocker block(m_machine);m_machine->clear();
    QStringList machines{m_fleet.local().host};machines.append(m_fleet.peerNames());
    if(!previous.isEmpty()&&!machines.contains(previous))machines.append(previous);
    if(const auto *project=m_projects.group(m_project->currentData().toString()))
        for(const auto &folder:project->folders)if(!machines.contains(folder.machine) && (folder.machineId.isEmpty() || folder.machine==m_fleet.local().host || m_fleet.peerNames().contains(folder.machine)))machines.append(folder.machine);
    for(const auto &machine:machines){
        if(machine.isEmpty())continue;
        const auto target=machine==m_fleet.local().host?QString():machine;
        const auto *box=target.isEmpty()?&m_fleet.local():m_fleet.peer(target);
        m_machine->addItem(machine+(box&&box->ok?QString():tr(" (offline)")),target);
    }
    m_machine->setCurrentIndex(qMax(0,m_machine->findData(previous)));
}

void NewSessionDialog::setGroups(const SessionOrganization &projects,const QString &selected)
{
    m_projects=projects;const QSignalBlocker block(m_project);m_project->clear();
    for(const auto &p:m_projects.groups())if(p.accessible)m_project->addItem(workspaceIcon("projects",QColor(p.color)),p.name,p.id);
    int index=m_project->findData(selected);if(index<0)index=m_project->findText(m_initialProject);
    if(index<0)index=m_project->findData(m_projects.defaultProject());m_project->setCurrentIndex(index);loadMachines();loadFolders();
}
bool NewSessionDialog::selectFolder(const QString &folderId)
{
    const auto *project=m_projects.group(m_project->currentData().toString());
    if(!project)return false;
    for(const auto &folder:project->folders)if(folder.id==folderId){
        const auto target=folder.machine==m_fleet.local().host?QString():folder.machine;
        const int machine=m_machine->findData(target);if(machine<0)return false;
        m_machine->setCurrentIndex(machine);
        const int index=m_folder->findData(folderId);if(index<0)return false;
        m_browsedFolders.remove(host());m_folder->setCurrentIndex(index);updateForm();return true;
    }
    return false;
}
void NewSessionDialog::loadFolders(const QString &selectedPath)
{
    const auto context=m_project->currentData().toString()+'\n'+host();
    // Refreshes retain the displayed folder, but only Browse choices cross projects.
    const auto previous=context==m_folderContext?m_folder->currentData(Qt::UserRole+2).toString():QString();
    m_folderContext=context;
    const QSignalBlocker block(m_folder);m_folder->clear();m_error->clear();
    const auto *project=m_projects.group(m_project->currentData().toString());
    const auto *machine=host().isEmpty()?&m_fleet.local():m_fleet.peer(host());
    const bool available=machine&&machine->ok;
    const auto add=[&](const QString &id,const QString &rawPath,const QString &name){
        const auto path=QDir::cleanPath(rawPath);
        m_folder->addItem(name.isEmpty()?(path=="/"?path:path.section('/',-1)):name,id);
        const int row=m_folder->count()-1;m_folder->setItemData(row,host(),Qt::UserRole+1);m_folder->setItemData(row,path,Qt::UserRole+2);
        m_folder->setItemData(row,available,Qt::UserRole+3);m_folder->setItemData(row,path,Qt::ToolTipRole);
    };
    if(project)for(const auto &folder:project->folders){
        if((folder.machine==m_fleet.local().host?QString():folder.machine)!=host())continue;
        add(folder.id,folder.path,folder.name);
    }
    const auto custom=!selectedPath.isEmpty()?selectedPath:!previous.isEmpty()?previous:m_browsedFolders.value(host());
    int index=-1;
    if(!custom.isEmpty()){
        index=m_folder->findData(custom,Qt::UserRole+2);
        if(index<0){add({},custom,{});index=m_folder->count()-1;}
    }
    m_folder->setCurrentIndex(index<0?0:index);
    loadAccounts();updateForm();updateWorktrees();
}
void NewSessionDialog::browseFolder()
{
    const auto target=host(),project=m_project->currentData().toString();
    const auto path=DirectoryDialog::chooseDirectory(m_client.executable(),target,m_folder->currentData(Qt::UserRole+2).toString(),this);
    if(path.isEmpty()||host()!=target||m_project->currentData().toString()!=project)return;
    selectPath(path);
    m_browsedFolders[target]=QDir::cleanPath(path);
}

void NewSessionDialog::updateForm()
{
    const auto path=m_folder->currentData(Qt::UserRole+2).toString();
    m_projectPath->setText(path);m_projectPath->setVisible(!path.isEmpty());
    m_projectPath->setToolTip(path);m_projectPath->ensurePolished();m_projectPath->setFixedHeight(m_projectPath->fontMetrics().height());
    const bool outside=!path.isEmpty()&&m_folder->currentData().toString().isEmpty();
    m_preview->setProperty("outsideProject",outside);
    m_preview->setStyleSheet(outside?QStringLiteral("color: %1;").arg(palette().color(QPalette::Window).lightness()<128?"#f0b65a":"#9b6300"):QString());
    m_preview->setText(path.isEmpty()?tr("Browse to choose a folder on this computer."):
        outside?tr("This folder is outside the project.\nStarting here will add it to this project."):
        tr("Folder in %1").arg(m_project->currentText()));
    m_preview->setToolTip(m_preview->text());m_preview->ensurePolished();m_preview->setFixedHeight(2*m_preview->fontMetrics().lineSpacing());
    const bool validName=!m_name->text().isEmpty()&&!m_name->text().contains(QRegularExpression("[\\s/.:]"));
    m_start->setEnabled(m_account->currentIndex()>=0&&!m_accountRequest&&!m_validation&&validName&&!path.isEmpty()&&m_folder->currentData(Qt::UserRole+3).toBool());
    for(auto *widget:QList<QWidget *>{m_project,m_machine,m_folder,m_agent,m_name,m_manage,m_worktrees})widget->setEnabled(!m_validation);
    const auto *machine=host().isEmpty()?&m_fleet.local():m_fleet.peer(host());
    m_browse->setEnabled(!m_validation&&machine&&machine->ok);
    m_newWorktree->setEnabled(!m_validation&&!m_catalogRequest&&machine&&machine->ok&&m_worktreeCatalog["state"]=="ok"
        &&!m_worktreeCatalog["stale"].toBool()&&!m_worktreeCatalog["common_dir"].toString().isEmpty());
    m_openTerminal->setEnabled(!m_validation&&m_agent->currentText()!="dsh");
    m_openTerminal->setToolTip(m_agent->currentText()=="dsh"?tr("DeepSeek opens in the workspace's Native UI."):tr("Also open an external terminal for this session."));
    m_account->setEnabled(!m_validation&&QStringList{"codex","claude","kimi"}.contains(m_agent->currentText()));
    const auto offline=tr("This computer is unavailable. Choose another folder or reconnect it.");
    if(!path.isEmpty()&&!m_folder->currentData(Qt::UserRole+3).toBool())m_error->setText(offline);
    else if(m_error->text()==offline)m_error->clear();
    const auto noAccount=tr("Restore or add an account in Agent accounts to start this agent.");
    if(m_account->count()==0)m_error->setText(noAccount);else if(m_error->text()==noAccount)m_error->clear();
    const auto loadingAccount=tr("Loading this session's account…"),missingAccount=tr("This session's account is unavailable. Choose another account.");
    if(m_accountPreset&&m_account->currentIndex()<0)m_error->setText(m_accountRequest?loadingAccount:missingAccount);
    else if(m_error->text()==loadingAccount||m_error->text()==missingAccount)m_error->clear();
}
void NewSessionDialog::start()
{
    if(!m_start->isEnabled())return;
    m_error->setText(tr("Checking folder…"));m_validation=m_client.requestDirectories(host(),m_folder->currentData(Qt::UserRole+2).toString());updateForm();
}
void NewSessionDialog::launch(const QString &target)
{
    QSettings().setValue("workspace/launchOpenTerminal",m_openTerminal->isChecked());
    emit launchRequested(host(),m_agent->currentText(),target,m_name->text(),m_account->currentData().toString(),m_project->currentData().toString(),m_openTerminal->isChecked()&&m_agent->currentText()!="dsh");accept();
}
void NewSessionDialog::loadAccounts()
{
    if(m_accountHost==host()&&(m_accountRequest||!m_accounts.isEmpty()))return;
    if(m_accountHost!=host()){m_removedAccounts={};m_accountPreset=false;}
    m_accountHost=host();m_accounts={};m_account->setToolTip({});updateAccounts();m_accountRequest=m_client.requestAccounts(host(),{"ls"});
}
void NewSessionDialog::updateAccounts()
{
    m_account->clear();
    bool removed=false;int defaultIndex=0;
    for(const auto &value:m_removedAccounts)if(value.toObject()["id"].toString()=="native-"+m_agent->currentText())removed=true;
    for(const auto &value:m_accounts){
        const auto a=value.toObject();if(a["provider"].toString()!=m_agent->currentText())continue;
        QString name=a["label"].toString();if(name=="Default account")name=tr("Native account");
        if(a["is_default"].toBool()){defaultIndex=m_account->count();name+=tr(" (default)");}
        m_account->addItem(name,a["id"].toString());
    }
    // Older/unavailable catalogs keep the native option explicit; an empty ID
    // would silently select a different configured HGS default.
    if((m_accounts.isEmpty()&&!removed)||(m_agent->currentText()=="dsh"&&m_account->count()==0))m_account->addItem(tr("Native account"),"native-"+m_agent->currentText());
    m_account->setCurrentIndex(m_accountPreset?m_account->findData(m_preferredAccount):defaultIndex);updateForm();
}

void NewSessionDialog::selectAccount(const QString &account)
{
    m_preferredAccount=account.isEmpty()?"native-"+m_agent->currentText():account;
    m_accountPreset=true;updateAccounts();
}

void NewSessionDialog::selectPath(const QString &path)
{
    if(!QDir::isAbsolutePath(path))return;
    m_browsedFolders.remove(host());
    loadFolders(QDir::cleanPath(path));
}
void NewSessionDialog::updateWorktrees()
{
    const auto path=m_folder->currentData(Qt::UserRole+2).toString();
    const auto *box=host().isEmpty()?&m_fleet.local():m_fleet.peer(host());
    const auto context=host()+'\n'+path+'\n'+QString::number(box&&box->ok);
    if(context==m_worktreeContext)return;m_worktreeContext=context;m_catalogRequest=0;
    m_worktreeCatalog=m_client.worktreeSnapshot(host(),path);
    m_worktrees->setVisible(!m_worktreeCatalog["worktrees"].toArray().isEmpty());m_newWorktree->setVisible(m_worktreeCatalog["state"]=="ok");
    if(!path.isEmpty()&&box&&box->ok)m_catalogRequest=m_client.requestWorktrees(host(),path);
    updateForm();
}
void NewSessionDialog::chooseWorktree()
{
    const auto machine=host(),project=m_project->currentData().toString();
    QDialog dialog(this);dialog.setObjectName("chooseWorktreeDialog");dialog.setWindowTitle(tr("Choose an existing worktree"));dialog.resize(620,420);
    auto *layout=new QVBoxLayout(&dialog);layout->setContentsMargins(20,18,20,18);
    auto *panel=new WorktreePanel(&m_client,true);layout->addWidget(panel);
    panel->setContext(machine,m_folder->currentData(Qt::UserRole+2).toString(),m_fleet);
    QString chosen;panel->launchRequested=[&](const QString &path){chosen=path;dialog.accept();};
    panel->cancelRequested=[&]{dialog.reject();};
    if(dialog.exec()==QDialog::Accepted&&!chosen.isEmpty()&&host()==machine&&m_project->currentData().toString()==project)selectPath(chosen);
}
void NewSessionDialog::createWorktree()
{
    if(!m_newWorktree->isEnabled())return;
    const auto machine=host(),project=m_project->currentData().toString();
    NewWorktreeDialog dialog(&m_client,machine,machine.isEmpty()?m_fleet.local().host:machine,
        m_folder->currentData(Qt::UserRole+2).toString(),m_worktreeCatalog,m_name->text(),this);
    if(dialog.exec()!=QDialog::Accepted||dialog.createdPath().isEmpty())return;
    if(host()!=machine||m_project->currentData().toString()!=project){
        m_error->setText(tr("Worktree created at %1 on %2. Select that folder to start a session.").arg(dialog.createdPath(),machine.isEmpty()?m_fleet.local().host:machine));return;
    }
    selectPath(dialog.createdPath());m_error->setText(tr("Worktree created. Ready to start the session."));
}
