#pragma once
#include <QDialog>
#include "HgsClient.h"

class QCheckBox;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;

// Native folder selection locally; the CLI browser accesses a peer over SSH.
class DirectoryDialog : public QDialog {
    Q_OBJECT
public:
    static QString chooseDirectory(const QString &hgsPath, const QString &host,
                                   const QString &path, QWidget *parent = nullptr);
    DirectoryDialog(const QString &hgsPath, const QString &host, const QString &path,
                    QWidget *parent = nullptr);
    QString directory() const { return m_current; }
protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
private:
    void navigate(const QString &path);
    HgsClient m_client;
    QString m_host, m_current, m_parent;
    quint64 m_request = 0;
    QLineEdit *m_path, *m_filter;
    QListWidget *m_list;
    QCheckBox *m_hidden;
    QLabel *m_status;
    QPushButton *m_choose;
};
