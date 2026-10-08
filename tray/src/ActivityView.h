#pragma once

#include <QJsonArray>
#include <QJsonObject>
#include <QHash>
#include <QSet>
#include <QWidget>

class QPushButton;
class QLabel;
class QTextBrowser;
class QUrl;
class QImage;

// Chronological, read-only journal presentation. Agent input lives in the
// composer's own widget; viewing activity never writes to a running session.
class ActivityView : public QWidget {
    Q_OBJECT
public:
    explicit ActivityView(QWidget *parent = nullptr);
    void setSessionKey(const QString &key);
    void setActivity(const QJsonObject &details, const QJsonArray &events,
                     const QString &fallbackPrompt = {}, bool tracked = true);
    void setTimeline(const QJsonObject &details, const QJsonArray &events,
                     const QJsonArray &localMessages, const QString &fallbackPrompt = {}, bool tracked = true);
    // The owner reconciles these local acknowledgements with arriving hooks.
    // Items: id, text, submitted_at, status, attachments (file references), error.
    void setLocalMessages(const QJsonArray &messages);
    void setTheme(bool dark);
    // Enlarges the transcript and queued input, not the overlay controls.
    void setContentScale(double scale);
    void showSearchResult(const QJsonObject &event, const QString &query);
    void clearSearchResult();
    QWidget *compactionIndicator() const { return m_compaction; }
    QPushButton *jumpButton() const { return m_latest; }
    QTextBrowser *browser() const { return m_browser; }
    bool replyVisible(const QString &replyId) const;
    void setAttachmentPreview(const QString &key, const QImage &image);

public slots:
    void jumpToLatest();

signals:
    // Files are routed to the session owner, which knows their machine and cwd.
    void fileReferenceActivated(const QString &href);
    void externalLinkActivated(const QUrl &url);
    void attachmentActivated(const QJsonObject &file);
    void attachmentPreviewRequested(const QString &key, const QJsonObject &file);
    void queueSendNowRequested(const QString &queueId);
    void messageActionRequested(const QString &id, const QString &action);
    void processRequested(const QString &id);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void render(bool contentUpdate);
    void updateJumpButton();
    void positionJumpButton();
    void activateLink(const QUrl &url);
    bool nearBottom() const;
    void scheduleFollow();

    QTextBrowser *m_browser;
    QWidget *m_queue;
    QLabel *m_queueText;
    QPushButton *m_queueSend;
    QPushButton *m_latest = nullptr;
    QWidget *m_compaction;
    QString m_sessionKey, m_conversation, m_html, m_fallbackPrompt;
    QJsonObject m_details;
    QJsonObject m_searchResult;
    QString m_searchQuery;
    QJsonArray m_events, m_localMessages;
    QHash<QString, bool> m_expanded;
    QHash<QString, QString> m_fileLinks;
    QHash<QString, QString> m_deliveryKeys;
    QHash<QString, QJsonObject> m_attachmentLinks;
    QHash<QString, QJsonObject> m_previewFiles;
    QHash<QString, QPair<QString, QString>> m_messageActions;
    QSet<QString> m_knownEvents, m_toggleKeys, m_processLinks;
    bool m_dark = false, m_tracked = true, m_initial = true;
    bool m_rendering = false, m_followLatest = true, m_followScheduled = false;
    int m_unseen = 0;
    double m_scale = 1.0;
};
