#pragma once
#include "UpdateController.h"
#include <QApplication>
#include <QCheckBox>
#include <QClipboard>
#include <QComboBox>
#include <QDesktopServices>
#include <QLabel>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QVBoxLayout>

class UpdatesWidget : public QWidget {
public:
    explicit UpdatesWidget(QWidget *parent=nullptr):QWidget(parent){
        auto *controller=UpdateController::instance();auto *layout=new QVBoxLayout(this);layout->setContentsMargins(0,0,0,0);
        auto addControl=[layout](QWidget *control){control->setSizePolicy(QSizePolicy::Maximum,QSizePolicy::Fixed);layout->addWidget(control,0,Qt::AlignLeft);};
        auto label=[layout]{auto *value=new QLabel;value->setWordWrap(true);value->setTextFormat(Qt::PlainText);value->setTextInteractionFlags(Qt::TextSelectableByMouse);layout->addWidget(value);return value;};
        auto *version=label();version->setText(tr("Installed desktop: %1").arg(QCoreApplication::applicationVersion()));
        auto *ownership=label();auto *released=label();auto *comparison=label();comparison->setObjectName("updatesComparison");auto *checked=label();auto *cache=label();auto *error=label();
        auto *automatic=new QCheckBox(tr("Check for updates automatically"));automatic->setObjectName("updatesAutomatic");addControl(automatic);
        auto *visibility=label();visibility->setText(tr("Available updates appear in the tray menu and the workspace status bar. New verified releases also show a system notification."));
        auto *channel=new QComboBox;channel->setObjectName("updatesChannel");channel->addItem(tr("Stable"),"stable");channel->addItem(tr("Nightly"),"nightly");addControl(channel);
        auto *check=new QPushButton(tr("Check for updates"));check->setObjectName("updatesCheck");addControl(check);
        auto *release=new QPushButton(tr("Open verified release"));addControl(release);
        auto *command=label();command->setObjectName("updatesCommand");
        auto *copy=new QPushButton;addControl(copy);auto *instructions=label();
        auto *source=new QPushButton(tr("Installation and source build instructions"));addControl(source);
        connect(source,&QPushButton::clicked,this,[]{QDesktopServices::openUrl(QUrl("https://github.com/ufna/zerus#installation"));});
        connect(automatic,&QCheckBox::toggled,controller,&UpdateController::setAutomaticChecks);
        connect(channel,&QComboBox::currentIndexChanged,this,[controller,channel]{if(controller->channel()!=channel->currentData().toString())controller->setChannel(channel->currentData().toString());});
        connect(check,&QPushButton::clicked,controller,&UpdateController::check);
        connect(release,&QPushButton::clicked,this,[controller]{if(controller->feed().valid)QDesktopServices::openUrl(QUrl(controller->feed().releaseUrl));});
        connect(copy,&QPushButton::clicked,this,[controller]{QString command=UpdateController::updateCommand(controller->installation(),controller->channel());if(!command.isEmpty())QApplication::clipboard()->setText(command);});
        auto refresh=[=]{auto install=controller->installation();QString details;
            if(install.kind=="package")details=tr("Package: %1\nInstalled package: %2").arg(install.package,install.version.isEmpty()?tr("unconfirmed"):install.version);
            else if(install.kind=="source")details=tr("Local build in a Git checkout; update and rebuild deliberately.");
            else if(install.kind=="checking")details=tr("Checking installation ownership…");else details=tr("Local / manually installed build; package ownership unconfirmed.");
            if(install.arch)details+=tr("\nSystem: Arch-based Linux");ownership->setText(details);
            auto feed=controller->feed();released->setText(feed.valid?tr("Released desktop: %1").arg(feed.version):tr("Released desktop: not checked"));
            comparison->setText(controller->busy()?tr("Checking…"):controller->comparison());comparison->setVisible(!comparison->text().isEmpty());
            checked->setText(tr("Last successful check: %1").arg(controller->lastChecked()));cache->setText(tr("Showing the last verified catalog."));cache->setVisible(controller->cached());error->setText(controller->checkError());error->setVisible(!error->text().isEmpty());
            {QSignalBlocker blocker(automatic);automatic->setChecked(controller->automaticChecks());}{QSignalBlocker blocker(channel);channel->setCurrentIndex(channel->findData(controller->channel()));}
            check->setEnabled(!controller->busy());release->setEnabled(feed.valid);
            QString value=UpdateController::updateCommand(install,controller->channel());bool optional=install.kind!="package";
            command->setText((optional?tr("Optional AUR installation: "):tr("Full system update: "))+value);command->setVisible(!value.isEmpty());copy->setText(optional?tr("Copy optional installation command"):tr("Copy update command"));copy->setVisible(!value.isEmpty());
            instructions->setText(optional?tr("This installs an AUR package; the current local build remains separate. Review the full system upgrade and your executable path."):tr("Run this through your AUR helper and review the full system upgrade before confirming."));if(value.isEmpty()&&install.arch)instructions->setText(tr("No supported AUR helper was found. See the installation instructions to choose an AUR helper."));instructions->setVisible(!value.isEmpty()||install.arch);
        };connect(controller,&UpdateController::changed,this,refresh);refresh();
    }
};
