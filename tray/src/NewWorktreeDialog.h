#pragma once
#include "DirectoryDialog.h"
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileInfo>
#include <QFormLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRegularExpression>
#include <QVBoxLayout>

// The source/machine are fixed for the whole operation. Creation is explicit;
// cancelling the parent launch never deletes a successfully created checkout.
class NewWorktreeDialog : public QDialog {
public:
    NewWorktreeDialog(HgsClient *client,const QString &host,const QString &machine,const QString &source,
        const QJsonObject &catalog,const QString &suggestedBranch,QWidget *parent=nullptr)
        :QDialog(parent),m_client(client),m_host(host),m_source(source),m_common(catalog["common_dir"].toString()) {
        setObjectName("newWorktreeDialog");setWindowTitle(tr("New worktree"));setMinimumWidth(580);resize(640,440);
        auto *layout=new QVBoxLayout(this);layout->setContentsMargins(24,22,24,22);layout->setSpacing(16);
        auto label=[&](const QString &text){auto *l=new QLabel(text);l->setTextFormat(Qt::PlainText);l->setWordWrap(true);return l;};
        auto *heading=label(tr("New worktree"));heading->setObjectName("heading");layout->addWidget(heading);
        auto *context=label(tr("Computer: %1\nRepository: %2").arg(machine,source));context->setTextInteractionFlags(Qt::TextSelectableByMouse);layout->addWidget(context);
        auto *form=new QFormLayout;form->setSpacing(12);form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
        m_branch=new QLineEdit(suggestedBranch);m_branch->setObjectName("newWorktreeBranch");m_branch->setPlaceholderText("feature/my-task");form->addRow(tr("New branch"),m_branch);
        m_base=new QComboBox;m_base->setObjectName("newWorktreeBase");m_base->setEditable(true);m_base->addItem("HEAD");
        for(const auto &value:catalog["worktrees"].toArray()){const auto branch=value.toObject()["branch"].toString();if(!branch.isEmpty()&&m_base->findText(branch)<0)m_base->addItem(branch);}
        m_base->setToolTip(tr("Local branch, tag, commit or an already fetched remote branch (for example origin/main)."));form->addRow(tr("Start from"),m_base);
        auto root=catalog["selected_root"].toString();if(root.isEmpty())root=source;
        m_parent=QFileInfo(root).path();m_repoName=QFileInfo(root).fileName();
        m_destination=new QLineEdit;m_destination->setObjectName("newWorktreePath");
        m_browse=new QPushButton(tr("Parent folder…"));m_browse->setAutoDefault(false);
        auto *row=new QHBoxLayout;row->setSpacing(8);row->addWidget(m_destination,1);row->addWidget(m_browse);form->addRow(tr("New folder"),row);
        layout->addLayout(form);
        layout->addWidget(label(tr("Creates a new branch and working folder. It remains on this computer if you cancel or cannot start the session.")));
        m_error=label({});m_error->setObjectName("newWorktreeError");layout->addWidget(m_error);layout->addStretch();
        auto *buttons=new QDialogButtonBox(QDialogButtonBox::Cancel);m_cancel=buttons->button(QDialogButtonBox::Cancel);
        m_create=buttons->addButton(tr("Create worktree"),QDialogButtonBox::AcceptRole);m_create->setObjectName("createWorktree");layout->addWidget(buttons);
        connect(buttons,&QDialogButtonBox::rejected,this,&QDialog::reject);
        connect(m_destination,&QLineEdit::textEdited,this,[this]{m_customPath=true;});
        connect(m_branch,&QLineEdit::textChanged,this,[this]{suggestPath();updateForm();});
        connect(m_base,&QComboBox::currentTextChanged,this,[this]{updateForm();});
        connect(m_destination,&QLineEdit::textChanged,this,[this]{updateForm();});
        connect(m_browse,&QPushButton::clicked,this,[this]{
            const auto folder=DirectoryDialog::chooseDirectory(m_client->executable(),m_host,QFileInfo(m_destination->text()).path(),this);
            if(folder.isEmpty())return;m_parent=folder;m_customPath=false;suggestPath();
        });
        connect(m_create,&QPushButton::clicked,this,[this]{
            m_error->setText(tr("Creating worktree…"));
            m_request=m_client->createWorktree(m_host,m_source,m_common,m_branch->text().trimmed(),m_base->currentText().trimmed(),m_destination->text());updateForm();
        });
        connect(m_client,&HgsClient::worktreeCreated,this,[this](quint64 request,bool ok,const QJsonObject &result,const QString &error){
            if(request!=m_request)return;m_request=0;
            if(ok){m_created=result["path"].toString();accept();}else{m_error->setText(error);updateForm();}
        });
        suggestPath();updateForm();m_branch->selectAll();m_branch->setFocus();
    }
    QString createdPath() const {return m_created;}
    void reject() override {if(!m_request)QDialog::reject();}
private:
    void suggestPath(){
        if(m_customPath)return;
        auto name=m_branch->text().trimmed();name.replace(QRegularExpression("[^\\p{L}\\p{N}._-]+"),"-");
        m_destination->setText(QDir(m_parent).filePath(m_repoName+'-'+name));
    }
    void updateForm(){
        for(auto *widget:QList<QWidget *>{m_branch,m_base,m_destination,m_browse,m_cancel})widget->setEnabled(!m_request);
        m_create->setEnabled(!m_request&&!m_branch->text().trimmed().isEmpty()&&!m_base->currentText().trimmed().isEmpty()&&QDir::isAbsolutePath(m_destination->text()));
    }
    HgsClient *m_client;QString m_host,m_source,m_common,m_parent,m_repoName,m_created;
    QLineEdit *m_branch,*m_destination;QComboBox *m_base;QLabel *m_error;QPushButton *m_browse,*m_cancel,*m_create;
    bool m_customPath=false;quint64 m_request=0;
};
