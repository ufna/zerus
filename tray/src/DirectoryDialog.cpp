#include "DirectoryDialog.h"
#include <QCheckBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPushButton>
#include <QVBoxLayout>

QString DirectoryDialog::chooseDirectory(const QString &hgsPath, const QString &host,
                                         const QString &path, QWidget *parent)
{
    if (host.isEmpty()) {
        QString start = path;
        if (start.isEmpty() || start == "~") start = QDir::homePath();
        else if (start.startsWith("~/")) start = QDir::homePath() + start.mid(1);
        return QFileDialog::getExistingDirectory(parent, tr("Choose a folder"), start);
    }
    DirectoryDialog browser(hgsPath, host, path, parent);
    return browser.exec() == QDialog::Accepted ? browser.directory() : QString();
}

DirectoryDialog::DirectoryDialog(const QString &hgsPath, const QString &host,
                                 const QString &path, QWidget *parent)
    : QDialog(parent), m_client(hgsPath, this), m_host(host)
{
    setObjectName("folderBrowser"); setWindowTitle(tr("Choose folder — %1").arg(host.isEmpty() ? tr("This machine") : host));
    resize(660, 540); setMinimumSize(500, 400);
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(24, 24, 24, 24); layout->setSpacing(14);
    auto *heading = new QLabel(tr("Choose a folder")); heading->setObjectName("detailTitle"); layout->addWidget(heading);
    auto *machine = new QLabel(tr("Folders on %1").arg(host.isEmpty() ? tr("this machine") : host));
    machine->setTextFormat(Qt::PlainText); machine->setObjectName("muted"); layout->addWidget(machine);
    auto *navigation = new QHBoxLayout;
    auto *up = new QPushButton(tr("Up")); up->setObjectName("folderUp"); navigation->addWidget(up);
    auto *home = new QPushButton(tr("Home")); navigation->addWidget(home);
    m_path = new QLineEdit; m_path->setObjectName("folderPath"); m_path->setAccessibleName(tr("Directory path")); navigation->addWidget(m_path, 1);
    auto *go = new QPushButton(tr("Go")); navigation->addWidget(go); layout->addLayout(navigation);
    m_filter = new QLineEdit; m_filter->setPlaceholderText(tr("Filter folders in this directory…")); m_filter->setClearButtonEnabled(true); layout->addWidget(m_filter);
    m_list = new QListWidget; m_list->setObjectName("folderList"); m_list->setAccessibleName(tr("Directories")); layout->addWidget(m_list, 1);
    m_list->installEventFilter(this);
    m_hidden = new QCheckBox(tr("Show hidden folders")); layout->addWidget(m_hidden);
    m_status = new QLabel; m_status->setWordWrap(true); m_status->setTextFormat(Qt::PlainText); m_status->setObjectName("muted"); layout->addWidget(m_status);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
    m_choose = buttons->addButton(tr("Use this folder"), QDialogButtonBox::AcceptRole); m_choose->setObjectName("chooseFolder");
    m_choose->setDefault(false); m_choose->setAutoDefault(false); layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(up, &QPushButton::clicked, this, [this]() { navigate(m_parent.isEmpty() ? "~" : m_parent); });
    connect(home, &QPushButton::clicked, this, [this]() { navigate("~"); });
    connect(go, &QPushButton::clicked, this, [this]() { navigate(m_path->text()); });
    connect(m_path, &QLineEdit::returnPressed, go, &QPushButton::click);
    connect(m_list, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem *item) { navigate(item->data(Qt::UserRole).toString()); });
    connect(m_list, &QListWidget::currentItemChanged, this, [this](QListWidgetItem *item) {
        if (m_current.isEmpty()) return;
        m_choose->setText(item ? tr("Use selected folder") : tr("Use this folder"));
    });
    connect(m_path, &QLineEdit::textEdited, this, [this]() {
        m_choose->setEnabled(false); m_status->setText(tr("Press Go to open this path."));
    });
    connect(m_choose, &QPushButton::clicked, this, [this]() {
        if (const auto *item = m_list->currentItem()) {
            m_current = item->data(Qt::UserRole).toString();
        }
        accept();
    });
    connect(m_hidden, &QCheckBox::toggled, this, [this]() { navigate(m_current.isEmpty() ? "~" : m_current); });
    connect(m_filter, &QLineEdit::textChanged, this, [this](const QString &query) {
        for (int i = 0; i < m_list->count(); ++i) {
            auto *item = m_list->item(i); item->setHidden(!item->text().contains(query, Qt::CaseInsensitive));
            if (item->isHidden() && item == m_list->currentItem()) m_list->setCurrentRow(-1);
        }
    });
    connect(&m_client, &HgsClient::directoriesReady, this, [this](quint64 request, const QString &, const QJsonObject &data) {
        if (request != m_request) return;
        m_current = data.value("path").toString(); m_parent = data.value("parent").toString();
        m_path->setText(m_current); m_list->clear(); m_filter->clear();
        for (const auto &value : data.value("directories").toArray()) {
            const auto folder = value.toObject(); auto *item = new QListWidgetItem(folder.value("name").toString(), m_list);
            item->setData(Qt::UserRole, folder.value("path").toString());
            item->setToolTip(folder.value("path").toString());
        }
        m_list->setEnabled(true); m_choose->setEnabled(true); m_choose->setText(tr("Use this folder"));
        m_status->setText(data.value("truncated").toBool() ? tr("Showing the first 2,000 folders. You can also enter an exact path above.") :
            m_list->count() ? tr("Double-click to open a folder, or select it and press Use selected folder.") : tr("No subfolders. You can use this folder."));
    });
    connect(&m_client, &HgsClient::directoriesFailed, this, [this](quint64 request, const QString &, const QString &error) {
        if (request != m_request) return;
        m_status->setText(error); m_current.clear(); m_list->clear(); m_choose->setEnabled(false);
    });
    for (auto *button : findChildren<QPushButton *>()) button->setAutoDefault(false);
    navigate(path.isEmpty() ? "~" : path);
}

bool DirectoryDialog::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_list && event->type() == QEvent::KeyPress) {
        const auto *key = static_cast<QKeyEvent *>(event);
        if (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter) {
            if (const auto *item = m_list->currentItem()) navigate(item->data(Qt::UserRole).toString());
            return true;
        }
    }
    return QDialog::eventFilter(watched, event);
}

void DirectoryDialog::navigate(const QString &path)
{
    m_path->setText(path); m_status->setText(tr("Loading folders…"));
    m_choose->setEnabled(false); m_list->setEnabled(false);
    m_request = m_client.requestDirectories(m_host, path, m_hidden->isChecked());
}
