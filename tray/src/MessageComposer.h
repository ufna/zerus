#pragma once

#include "HgsClient.h"
#include <QHash>
#include <QWidget>

class QLabel;
class QComboBox;
class QPlainTextEdit;
class QPushButton;
class QScrollArea;
class QHBoxLayout;
class QFrame;

// Drafts belong to a selected session, never to the visible row position.
// A failed or uncertain delivery keeps text and attachments for recovery.
class MessageComposer : public QWidget {
    Q_OBJECT
public:
    explicit MessageComposer(QWidget *parent = nullptr);
    void setSessionKey(const QString &key);
    void setAvailability(bool available, const QString &reason = {});
    void setInterruptAvailability(bool working, bool enabled, const QString &reason = {});
    // Same as Stop; false when no turn can be interrupted now.
    bool requestInterrupt();
    // Fills an empty draft, for example with the prompt of an interrupted turn.
    bool offerDraft(const QString &key, const QString &text);
    void setTheme(bool dark);
    // Enlarges the message field; its actions keep the workspace size.
    void setContentScale(double scale);
    void setModelSettings(const QString &model, const QString &effort, const QJsonArray &options,
                          bool enabled, const QString &reason, const QString &pendingModel = {},
                          const QString &pendingEffort = {}, const QString &applyWhen = {});
    void setSettingsTerminalAvailable(bool available);
    void setSending(const QString &key, bool preserveDraft = false);
    bool draftMatches(const QString &key, const QString &text, const QList<MessageAttachment> &attachments) const;
    void deliveryFinished(const QString &key, bool ok, const QString &detail = {}, bool uncertain = false);
    void renameDraft(const QString &oldKey, const QString &newKey);
    bool isSending(const QString &key) const;
    bool addAttachment(const QString &name, const QString &mime, const QByteArray &data);
    bool canAttachFiles() const { return !m_key.isEmpty() && !isSending(m_key); }
    QString sessionKey() const { return m_key; }
    void attachDroppedFiles(const QStringList &paths);
    QPlainTextEdit *editor() const { return m_editor; }

signals:
    void interruptRequested(const QString &sessionKey);
    void settingsTerminalRequested(const QString &sessionKey);
    void settingsRequested(const QString &sessionKey, const QString &model, const QString &effort);
    void sendRequested(const QString &sessionKey, const QString &text, const QList<MessageAttachment> &attachments);

protected:
    void resizeEvent(QResizeEvent *event) override;

private:
    struct Draft {
        QString text, notice;
        QList<MessageAttachment> attachments;
        int nextAttachmentNumber = 1;
        int position = 0, anchor = 0;
        bool sending = false, error = false, uncertain = false;
    };
    void send();
    void restoreDraft();
    void attachFiles();
    void attachFile(const QString &path);
    void updateControls();
    void rebuildAttachments();
    void showError(const QString &text);
    void openSettings();
    void updateSettingsButton();
    void updateSettingsEfforts();
    void updateSettingsApply();

    QHash<QString, Draft> m_drafts;
    QHash<QString, Draft> m_preservedDrafts;
    QString m_key, m_unavailableReason;
    bool m_available = false, m_loading = false, m_dark = true;
    double m_scale = 1.0;
    QPlainTextEdit *m_editor;
    QLabel *m_status;
    QFrame *m_settingsPopup;
    QComboBox *m_models, *m_efforts;
    QLabel *m_settingsHint;
    QLabel *m_modelLabel, *m_effortLabel;
    QPushButton *m_settingsTerminal;
    bool m_settingsTerminalAvailable = false;
    QPushButton *m_settings, *m_applySettings;
    QJsonArray m_modelOptions;
    QString m_model, m_effort, m_pendingModel, m_pendingEffort, m_settingsReason, m_applyWhen, m_popupKey;
    bool m_settingsEnabled = false;
    QPushButton *m_attach, *m_send, *m_retry;
    QPushButton *m_stop;
    QScrollArea *m_attachmentScroll;
    QWidget *m_attachmentList;
    QHBoxLayout *m_attachmentsLayout;
    QHBoxLayout *m_actions;
    QHBoxLayout *m_feedback;
    bool m_narrow = false;
};
