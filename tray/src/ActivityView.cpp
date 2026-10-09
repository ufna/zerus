#include "ActivityView.h"
#include "ContentScale.h"
#include "MarkdownHtml.h"
#include "MarkdownObjects.h"
#include "ProcessSettings.h"
#include "SessionFileReference.h"

#include <QDateTime>
#include <QAbstractTextDocumentLayout>
#include <QCryptographicHash>
#include <QDesktopServices>
#include <QEvent>
#include <QElapsedTimer>
#include <QPainter>
#include <QHideEvent>
#include <QShowEvent>
#include <QImage>
#include <QJsonDocument>
#include <QLabel>
#include <QMimeData>
#include <QProgressBar>
#include <QHBoxLayout>
#include <QPushButton>
#include <QScrollBar>
#include <QTextBlock>
#include <QTextBrowser>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextFragment>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>

namespace {
// A slow, smooth cycle independent of the platform's busy-bar speed.
class CompactionProgress final : public QProgressBar {
public:
    explicit CompactionProgress(QWidget *parent) : QProgressBar(parent) {
        m_timer.setInterval(40);
        connect(&m_timer, &QTimer::timeout, this, qOverload<>(&QWidget::update));
    }
protected:
    void showEvent(QShowEvent *event) override {
        QProgressBar::showEvent(event); m_elapsed.start(); m_timer.start();
    }
    void hideEvent(QHideEvent *event) override {
        m_timer.stop(); QProgressBar::hideEvent(event);
    }
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this); painter.setRenderHint(QPainter::Antialiasing);
        painter.setPen(Qt::NoPen); painter.setBrush(palette().color(QPalette::Base));
        painter.drawRoundedRect(rect(), 2.5, 2.5);
        const qreal phase = (m_elapsed.isValid() ? m_elapsed.elapsed() % 3200 : 0) / 3200.;
        const qreal position = (1 - std::cos(phase * 2 * 3.141592653589793)) / 2;
        const qreal chunk = width() * .4;
        painter.setBrush(palette().color(QPalette::Highlight));
        painter.drawRoundedRect(QRectF(position * (width() - chunk), 0, chunk, height()), 2.5, 2.5);
    }
private:
    QElapsedTimer m_elapsed;
    QTimer m_timer;
};

// QTextDocument can otherwise read local files as well as network resources.
// Hook text is display data, never a source of images, CSS or local file reads.
class JournalDocument final : public QTextDocument {
public:
    using QTextDocument::QTextDocument;
    qreal pixelRatio = 1.0;
protected:
    // Only drawn Markdown decorations load; remote and file resources never do.
    QVariant loadResource(int, const QUrl &url) override { return MarkdownObjects::resource(url, pixelRatio); }
};

// Copies what the reader sees instead of U+FFFC for chips and markers.
class ActivityBrowser final : public QTextBrowser {
public:
    using QTextBrowser::QTextBrowser;
protected:
    QMimeData *createMimeDataFromSelection() const override
    {
        auto *data = new QMimeData;
        data->setText(MarkdownObjects::plainText(textCursor()));
        return data;
    }
};

bool webLink(const QUrl &url)
{
    return url.isValid() && !url.host().isEmpty() && url.userInfo().isEmpty()
        && (url.scheme() == "https" || url.scheme() == "http");
}

QString escaped(const QString &text) { return text.toHtmlEscaped(); }

QString userMessage(const QString &text)
{
    // The composer accepts plain text. Markdown would consume quote markers,
    // list numbers and line breaks that are part of the user's actual request.
    return "<p style='white-space:pre-wrap;'>" + escaped(text) + "</p>";
}

QJsonArray questionReplies(QString text)
{
    text = text.trimmed();
    if (text.startsWith("# Context from my IDE setup:\n")) {
        const QString marker = "\n## My request for Codex:\n";
        const auto request = text.lastIndexOf(marker);
        if (request < 0) return {};
        text = text.mid(request + marker.size()).trimmed();
    }
    const QString start = "<send_user_message_question_reply>";
    const QString end = "</send_user_message_question_reply>";
    if (!text.startsWith(start) || !text.endsWith(end)) return {};
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(text.mid(start.size(), text.size() - start.size() - end.size()).toUtf8(), &error);
    if (error.error != QJsonParseError::NoError) return {};
    const auto replies = document.isObject() ? QJsonArray{document.object()} : document.array();
    for (const auto &value : replies) {
        if (!value.isObject()) return {};
        const auto reply = value.toObject();
        if (!reply.value("questionItemId").isString() || reply.value("questionItemId").toString().isEmpty()
            || !reply.value("question").isString() || reply.value("question").toString().trimmed().isEmpty()
            || !reply.value("answer").isString()) return {};
    }
    return replies;
}

QString questionReplyBody(const QJsonArray &replies, const QString &muted, const QString &accent,
                          const QString &surface, const QString &border)
{
    QString html;
    for (const auto &value : replies) {
        const auto reply = value.toObject();
        html += QString("<p style='font-size:10px;color:%1;margin-bottom:3px;'>%2</p>"
            "<p style='color:%1;white-space:pre-wrap;margin-top:0;margin-bottom:10px;'>%3</p>"
            "<table width='100%' cellspacing='0' cellpadding='10' style='border:1px solid %4;'><tr><td bgcolor='%5'>"
            "<p style='font-size:10px;color:%6;margin-top:0;margin-bottom:4px;'><b>%7</b></p>"
            "<p style='white-space:pre-wrap;margin:0;'>%8</p></td></tr></table>")
            .arg(muted, escaped(QObject::tr("Question")), escaped(reply.value("question").toString()),
                 border, surface, accent, escaped(QObject::tr("Your answer")), escaped(reply.value("answer").toString()));
    }
    return html;
}

QString eventKey(const QJsonObject &event)
{
    if (!event.value("activity_key").toString().isEmpty()) return event.value("activity_key").toString();
    if(event.value("type")=="LocalMessage")return "local-"+event.value("id").toString();
    if(event.value("source")=="hgs_delivery")return "receipt-"+event.value("message_id").toString();
    if (event.value("source") == "kimi_wire" && !event.value("message_id").toString().isEmpty())
        return "kimi-" + event.value("message_id").toString();
    const QString nativeReply = event.value("reply_id").toString();
    if (nativeReply.startsWith("dsh-turn-") && nativeReply.mid(9).toLongLong() >= 0 && event.value("source") == "dsh_native") return nativeReply;
    if (event.value("seq").toInteger() > 0) return QString::number(event.value("seq").toInteger());
    return QString::fromLatin1(QCryptographicHash::hash(QJsonDocument(event).toJson(QJsonDocument::Compact),
                                                       QCryptographicHash::Sha256).toHex().left(20));
}

QString eventTitle(const QString &type)
{
    static const QMap<QString, QString> titles{
        {"SessionStart", "Session started"}, {"SessionEnd", "Session closed"},
        {"TurnStarted", "Turn started"}, {"UserPromptQueued", "Message queued"},
        {"PreToolUse", "Tool started"}, {"PostToolUse", "Tool finished"},
        {"PostToolUseFailure", "Tool failed"}, {"PermissionRequest", "Approval requested"},
        {"PermissionResult", "Approval resolved"}, {"Stop", "Response finished"},
        {"StopFailure", "Response failed"}, {"Interrupt", "Turn interrupted"},
        {"PreCompact", "Compacting context"}, {"PostCompact", "Context compacted"},
        {"SubagentStart", "Subagent started"}, {"SubagentStop", "Subagent finished"},
        {"TaskStarted", "Background task started"}, {"Notification", "Agent notification"}};
    return titles.value(type, type.isEmpty() ? QObject::tr("Activity") : type);
}

QString timeText(const QJsonObject &event)
{
    const double at = event.value("at").toDouble();
    if (at <= 0) return {};
    return QDateTime::fromSecsSinceEpoch(qint64(at)).toLocalTime().toString("d MMM HH:mm:ss");
}

QString messageRole(const QJsonObject &event)
{
    if (!event.value("agent_id").toString().isEmpty() || (event.value("detail").toString().isEmpty() && event.value("attachments").toArray().isEmpty())) return {};
    // Claude starts a turn for finished background work; nobody wrote it.
    const auto origin = event.value("origin").toString();
    if (origin == "task_notification" || origin == "subagent_report" || origin == "peer_message") return "notice";
    const auto type = event.value("type").toString();
    // Kimi reports an empty UserPromptSubmit followed by the actual prompt in
    // TurnStarted. Keep that prompt in the timeline, ahead of its tool calls.
    if (type == "UserPromptSubmit" || type == "UserPromptQueued" || type == "TurnStarted" || type == "QuestionAnswered" || type=="LocalMessage") return "user";
    if (type == "Stop" || type == "AgentMessage") return "assistant";
    return {};
}

bool attention(const QJsonObject &event)
{
    const auto type = event.value("type").toString();
    return type == "PermissionRequest" || type == "PostToolUseFailure" || type == "StopFailure" || type == "Interrupt";
}

// A snapshot contains a longer version of the latest journal excerpt. Expand
// only a matching prefix; never attribute an older response to a newer turn.
bool isExcerptOf(QString excerpt, const QString &full)
{
    if (excerpt.endsWith(QChar(0x2026))) excerpt.chop(1);
    return !excerpt.isEmpty() && full.startsWith(excerpt);
}

QString markdown(const QString &text, const MarkdownTheme &theme, QHash<QString, QString> &fileLinks)
{
    return MarkdownHtml::render(text, theme, [&fileLinks](const QString &href) -> MarkdownLink {
        const auto file = SessionFileReference::parse(href);
        if (file.valid()) {
            const QString key = QString::fromLatin1(QCryptographicHash::hash(href.toUtf8(), QCryptographicHash::Sha256).toHex());
            fileLinks.insert(key, href);
            return {"hgs-file:" + key, file.path + file.location, file.path + file.location};
        }
        if (webLink(QUrl(href))) return {href, {}, {}};
        return {};
    });
}

// Named anchors let a reading position survive journal pruning and expansion
// above it. They also preserve both endpoints of a text selection on refresh.
QMap<QString, int> bookmarks(QTextDocument *document)
{
    QMap<QString, int> result;
    for (auto block = document->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto f = it.fragment();
            for (const auto &name : f.charFormat().anchorNames())
                if (name.startsWith("item-")) result.insert(name, f.position());
        }
    return result;
}

struct Position {
    QString key;
    int offset = 0, absolute = 0;
    Position(int position, const QMap<QString, int> &anchors) : absolute(position) {
        int nearest = -1;
        for (auto it = anchors.cbegin(); it != anchors.cend(); ++it)
            if (it.value() <= position && it.value() > nearest) { key = it.key(); nearest = it.value(); }
        offset = nearest < 0 ? position : position - nearest;
    }
    int restored(const QMap<QString, int> &anchors, int length) const {
        return qBound(0, anchors.contains(key) ? anchors.value(key) + offset : absolute, qMax(0, length - 1));
    }
};
}

ActivityView::ActivityView(QWidget *parent) : QWidget(parent)
{
    setObjectName("activityView");
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 0, 0, 0); layout->setSpacing(5);
    m_browser = new ActivityBrowser(this); m_browser->setObjectName("activity");
    m_browser->setDocument(new JournalDocument(m_browser));
    m_browser->setFrameShape(QFrame::NoFrame);
    m_browser->setOpenLinks(false); m_browser->setOpenExternalLinks(false);
    m_browser->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_browser->setWordWrapMode(QTextOption::WrapAtWordBoundaryOrAnywhere);
    m_browser->setAccessibleName(tr("Session activity"));
    m_browser->viewport()->installEventFilter(this);
    layout->addWidget(m_browser, 1);
    // A browser child overlays the viewport without participating in layout
    // or moving with its scrolled contents.
    m_latest = new QPushButton(m_browser); m_latest->setObjectName("activityJumpLatest");
    m_latest->setAccessibleName(tr("Jump to latest activity"));
    m_latest->hide();
    m_compaction = new QWidget(this); m_compaction->setObjectName("activityCompaction");
    m_compaction->setAttribute(Qt::WA_StyledBackground);
    auto *progressRow = new QHBoxLayout(m_compaction); progressRow->setContentsMargins(12, 3, 12, 3); progressRow->setSpacing(10);
    auto *progress = new CompactionProgress(m_compaction); progress->setObjectName("compactionProgress");
    progress->setRange(0, 0); progress->setTextVisible(false); progress->setFixedSize(48, 5);
    progress->setAccessibleName(tr("Compacting context"));
    auto *caption = new QLabel(tr("Compacting context…"), m_compaction); caption->setObjectName("compactionCaption");
    caption->setTextFormat(Qt::PlainText);
    progressRow->addWidget(progress); progressRow->addWidget(caption); progressRow->addStretch();
    m_compaction->setToolTip(tr("The agent is compressing conversation context."));
    layout->addWidget(m_compaction); m_compaction->hide();
    m_queue = new QWidget(this); m_queue->setObjectName("activityInputQueue"); m_queue->setAttribute(Qt::WA_StyledBackground);
    auto *queueLayout = new QVBoxLayout(m_queue); queueLayout->setContentsMargins(12,8,12,8);
    auto *queueHeader = new QHBoxLayout;
    auto *queueCaption = new QLabel(tr("Queued in agent")); queueCaption->setObjectName("queueCaption");
    queueHeader->addWidget(queueCaption); queueHeader->addStretch();
    m_queueSend = new QPushButton(tr("Send now")); m_queueSend->setObjectName("queueSendNow"); queueHeader->addWidget(m_queueSend);
    queueLayout->addLayout(queueHeader);
    m_queueText = new QLabel; m_queueText->setObjectName("queueText"); m_queueText->setTextFormat(Qt::PlainText); m_queueText->setWordWrap(true);
    m_queueText->setTextInteractionFlags(Qt::TextSelectableByMouse); m_queueText->setMaximumHeight(100);
    queueLayout->addWidget(m_queueText); layout->addWidget(m_queue); m_queue->hide();
    connect(m_queueSend, &QPushButton::clicked, this, [this] { emit queueSendNowRequested(m_details["input_queue"].toObject()["id"].toString()); });
    connect(m_latest, &QPushButton::clicked, this, &ActivityView::jumpToLatest);
    connect(m_browser, &QTextBrowser::anchorClicked, this, &ActivityView::activateLink);
    connect(this, &ActivityView::externalLinkActivated, this, [](const QUrl &url) { QDesktopServices::openUrl(url); });
    connect(m_browser->verticalScrollBar(), &QScrollBar::valueChanged, this, [this] {
        if (m_rendering) return;
        m_followLatest = nearBottom() && !m_browser->textCursor().hasSelection();
        if (m_followLatest) m_unseen = 0;
        updateJumpButton();
    });
    connect(m_browser->verticalScrollBar(), &QScrollBar::rangeChanged, this, [this] {
        if (!m_rendering && m_followLatest && !m_browser->textCursor().hasSelection()) scheduleFollow();
    });
    connect(m_browser, &QTextBrowser::selectionChanged, this, [this] {
        if (!m_rendering && m_browser->textCursor().hasSelection()) m_followLatest = false;
    });
    m_relayout = new QTimer(this); m_relayout->setSingleShot(true); m_relayout->setInterval(150);
    connect(m_relayout, &QTimer::timeout, this, [this] {
        if (m_html.isEmpty()) return;
        m_html.clear(); render(false);
    });
    setTheme(false);
}

void ActivityView::setSessionKey(const QString &key)
{
    if (key == m_sessionKey) return;
    m_sessionKey = key; m_conversation.clear(); m_expanded.clear(); m_knownEvents.clear();
    m_attachmentLinks.clear();m_fileLinks.clear();
    m_previewFiles.clear(); m_deliveryKeys.clear();
    m_messageActions.clear();
    m_localMessages = {}; m_searchResult = {}; m_searchQuery.clear(); m_browser->setExtraSelections({});
    m_initial = true; m_followLatest = true; m_unseen = 0; m_html.clear();
    m_browser->clear(); updateJumpButton();
    m_compaction->hide(); m_queue->hide();
}

void ActivityView::setActivity(const QJsonObject &details, const QJsonArray &events, const QString &fallbackPrompt, bool tracked)
{
    const auto conversation = details.value("conversation_id").toString();
    if (!m_conversation.isEmpty() && !conversation.isEmpty() && conversation != m_conversation) {
        m_previewFiles.clear(); m_deliveryKeys.clear();
        m_searchResult = {}; m_searchQuery.clear(); m_browser->setExtraSelections({});
        m_expanded.clear(); m_knownEvents.clear(); m_initial = true; m_followLatest = true; m_unseen = 0; m_html.clear();
    }
    if (!conversation.isEmpty()) m_conversation = conversation;
    m_details = details; m_events = events; m_fallbackPrompt = fallbackPrompt; m_tracked = tracked;
    for(const auto &value:details.value("attachment_messages").toArray()) {
        const auto receipt=value.toObject();if(receipt.value("source")!="hgs_delivery" || receipt.value("attachments").toArray().isEmpty())continue;
        const auto submitted=receipt.value("submitted_text").toString().trimmed();int match=-1;double nearest=10.;
        for(int i=0;i<m_events.size();++i) {
            const auto event=m_events[i].toObject();const auto body=event.value("detail").toString().trimmed();
            const double distance=qAbs(event.value("at").toDouble()-receipt.value("at").toDouble());
            if(!event.value("agent_id").toString().isEmpty() || !QStringList{"UserPromptSubmit","UserPromptQueued","TurnStarted"}.contains(event.value("type").toString()))continue;
            if(event.value("attachments").toArray().isEmpty() && distance<=nearest && !submitted.isEmpty() && !body.isEmpty() && (body.startsWith(submitted.left(1000)) || submitted.startsWith(body.left(1000)))){match=i;nearest=distance;}
        }
        if(match<0){auto message=receipt;message["attachment_submitted_text"]=receipt.value("submitted_text");m_events.append(message);}
        else {auto event=m_events[match].toObject();event["detail"]=receipt.value("detail");event["attachments"]=receipt.value("attachments");event["attachment_request_id"]=receipt.value("message_id");event["attachment_submitted_text"]=receipt.value("submitted_text");m_events[match]=event;}
    }
    for (const auto &value : details.value("provider_messages").toArray()) {
        const auto message = value.toObject();
        if(message.value("type")=="AgentThinking"&&message.value("source")=="claude_transcript") {
            m_events.append(message);continue;
        }
        if (message.value("type") != "AgentMessage" || !QStringList{"kimi_wire", "codex_transcript", "claude_transcript"}.contains(message.value("source").toString())) continue;
        bool matched = false;
        for (int i = 0; i < m_events.size(); ++i) {
            auto event = m_events[i].toObject();
            if (event.value("type") != "Stop" || !event.value("agent_id").toString().isEmpty()
                || qAbs(event.value("at").toDouble() - message.value("at").toDouble()) > 2.0) continue;
            const QString excerpt = event.value("detail").toString();
            // A progress message just before Stop is not the final response.
            // Keep it at its original position instead of filling an empty Stop.
            if (excerpt.isEmpty() && message.value("message_phase") == "commentary") continue;
            if (!excerpt.isEmpty() && !isExcerptOf(excerpt, message.value("detail").toString())) continue;
            event["detail"] = message.value("detail"); event["source"] = message.value("source");
            m_events[i] = event; matched = true; break;
        }
        if (!matched) m_events.append(message);
    }
    for (const auto &value : details.value("provider_errors").toArray()) {
        const auto error = value.toObject();
        if (error.value("type") == "StopFailure" && error.value("source") == "codex_native_log") m_events.append(error);
    }
    render(true);
}

void ActivityView::setLocalMessages(const QJsonArray &messages)
{
    if (m_localMessages == messages) return;
    m_localMessages = messages; render(true);
}

void ActivityView::setTimeline(const QJsonObject &details, const QJsonArray &events,
                               const QJsonArray &localMessages, const QString &fallbackPrompt, bool tracked)
{
    // A poll can replace local delivery cards with native events. Commit both
    // inputs together so the document never contains both generations.
    m_localMessages = localMessages;
    setActivity(details, events, fallbackPrompt, tracked);
}

void ActivityView::setTheme(bool dark)
{
    m_dark = dark;
    auto background = this->palette();
    background.setColor(QPalette::Window, QColor(dark ? "#1c2229" : "#ffffff"));
    setPalette(background); setAutoFillBackground(true);
    QPalette palette = m_browser->palette();
    palette.setColor(QPalette::Base, QColor(dark ? "#1c2229" : "#ffffff"));
    palette.setColor(QPalette::Text, QColor(dark ? "#e8edf4" : "#1a2733"));
    palette.setColor(QPalette::Highlight, QColor(dark ? "#375e54" : "#b6e2d2"));
    palette.setColor(QPalette::HighlightedText, palette.color(QPalette::Text));
    m_browser->setPalette(palette);
    m_latest->setStyleSheet(QString("QPushButton { color:%1; background:%2; border:1px solid %3; border-radius:12px; padding:5px 13px; font-size:11px; } QPushButton[footer=true] { padding:0 10px; min-height:22px; max-height:22px; } QPushButton:hover { border-color:%1; }")
        .arg(dark ? "#8bdfc0" : "#167357", dark ? "#233a35" : "#e7f3ed", dark ? "#456e61" : "#a5c8b8"));
    m_compaction->setStyleSheet(QString("QWidget#activityCompaction { background:%3; } QLabel { color:%1; background:transparent; font-size:12px; } QProgressBar { background:%2; border:0; border-radius:2px; } QProgressBar::chunk { background:%1; border-radius:2px; }")
        .arg(dark ? "#8bdfc0" : "#167357", dark ? "#34404a" : "#dbe3e9", dark ? "#1c2229" : "#ffffff"));
    m_queue->setStyleSheet(QString("QWidget#activityInputQueue{background:%1;border:1px solid %2;border-radius:7px;} QLabel#queueCaption{color:%3;font-weight:600;}")
        .arg(dark ? "#2d2c23" : "#fff7e4", dark ? "#655638" : "#d7be84", dark ? "#edbd77" : "#805516")
        + (qFuzzyCompare(m_scale, 1.0) ? QString() : QString(" QLabel#queueText{font-size:%1px;}").arg(ContentScale::px(13, m_scale))));
    auto *progress = m_compaction->findChild<QProgressBar *>("compactionProgress");
    auto progressPalette = progress->palette();
    progressPalette.setColor(QPalette::Base, QColor(dark ? "#34404a" : "#dbe3e9"));
    progressPalette.setColor(QPalette::Highlight, QColor(dark ? "#8bdfc0" : "#167357"));
    progress->setPalette(progressPalette);
    render(false);
}

void ActivityView::setContentScale(double scale)
{
    scale = ContentScale::clamp(scale);
    if (qFuzzyCompare(scale, m_scale)) return;
    m_scale = scale;
    // Explicit HTML lengths are scaled while rendering. Markdown headings are
    // relative to the document's default font, which follows the widget font.
    m_browser->setStyleSheet(qFuzzyCompare(scale, 1.0) ? QString()
        : QString("QTextBrowser { font-size:%1px; }").arg(ContentScale::px(13, scale)));
    m_queueText->setMaximumHeight(ContentScale::px(100, scale));
    setTheme(m_dark);
}

QString ActivityView::plainText() const
{
    QTextCursor all(m_browser->document());
    all.select(QTextCursor::Document);
    return MarkdownObjects::plainText(all);
}

bool ActivityView::replyVisible(const QString &replyId) const
{
    if (!m_searchResult.isEmpty() || !nearBottom() || m_rendering || m_followScheduled) return false;
    const auto anchors = bookmarks(m_browser->document());
    const auto key = "item-" + replyId.section(':', 0, 0);
    int start = anchors.value(key, -1);
    if (start < 0 && !m_details.value("last_message").toString().isEmpty()) start = anchors.value("item-answer-snapshot", -1);
    if (start < 0) {
        // Providers can finish a turn without response text. Its visible Ready
        // state and final operational event are the available result.
        for (const auto &value : m_events) {
            const auto event = value.toObject();
            if (QString::number(event.value("seq").toInteger()) == replyId.section(':', 0, 0)
                && event.value("type") == "Stop" && event.value("detail").toString().isEmpty()
                && m_details.value("activity") == "idle") return true;
        }
        // Catching up to activity after a completed turn acknowledges that
        // turn too, even after its event was pruned from the visible window.
        bool numeric=false;const auto sequence=replyId.section(':',0,0).toLongLong(&numeric);
        if(numeric&&sequence>0&&m_details.value("reply_id").toString()==replyId)
            for(const auto &value:m_events) if(value.toObject().value("seq").toInteger()>=sequence)return true;
        return false;
    }
    QTextCursor cursor(m_browser->document()); cursor.setPosition(start);
    return m_browser->cursorRect(cursor).top() < m_browser->viewport()->height();
}

bool ActivityView::nearBottom() const
{
    auto *bar = m_browser->verticalScrollBar();
    return bar->maximum() - bar->value() <= 30;
}

void ActivityView::scheduleFollow()
{
    if (m_followScheduled) return;
    m_followScheduled = true;
    QTimer::singleShot(0, this, [this] {
        m_followScheduled = false;
        if (m_followLatest && !m_browser->textCursor().hasSelection()) jumpToLatest();
    });
}

void ActivityView::showSearchResult(const QJsonObject &event, const QString &query)
{
    m_searchResult = event; m_searchQuery = query;
    m_initial = false; m_followLatest = false; m_html.clear();
    m_expanded.insert("group-" + eventKey(event), true);
    render(false);
}

void ActivityView::clearSearchResult()
{
    if (m_searchResult.isEmpty()) return;
    m_searchResult = {}; m_searchQuery.clear(); m_browser->setExtraSelections({});
    m_html.clear(); m_initial = true; m_followLatest = true; render(false);
}

void ActivityView::jumpToLatest()
{
    // Hiding a focused button would let Qt focus the next control (context
    // usage). Keep explicit navigation in the journal, without taking focus
    // from the composer during automatic following.
    if (m_latest->hasFocus()) m_browser->setFocus(Qt::OtherFocusReason);
    if (!m_searchResult.isEmpty()) { clearSearchResult(); return; }
    m_rendering = true;
    auto cursor = m_browser->textCursor(); cursor.clearSelection(); m_browser->setTextCursor(cursor);
    m_followLatest = true; m_unseen = 0; m_latest->hide();
    auto *bar = m_browser->verticalScrollBar(); bar->setValue(bar->maximum());
    m_rendering = false;
}

void ActivityView::updateJumpButton()
{
    if (!m_searchResult.isEmpty()) {
        m_latest->setText(tr("Back to latest activity")); m_latest->show(); positionJumpButton(); return;
    }
    m_latest->setText(m_unseen > 0 ? tr("↓  %1 new — Jump to latest").arg(m_unseen) : tr("↓  Jump to latest"));
    m_latest->setVisible(!m_followLatest && m_browser->verticalScrollBar()->maximum() > 0);
    positionJumpButton();
}

void ActivityView::positionJumpButton()
{
    if (!m_latest || m_latest->parentWidget()!=m_browser) return;
    const auto viewport = m_browser->viewport()->geometry();
    const auto size = m_latest->sizeHint().boundedTo(QSize(qMax(0,viewport.width()-16),viewport.height()));
    m_latest->resize(size);
    m_latest->move(viewport.left()+(viewport.width()-size.width())/2,
        qMax(viewport.top(),viewport.bottom()+1-size.height()-8));
    m_latest->raise();
}

bool ActivityView::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_browser->viewport() && (event->type() == QEvent::Resize || event->type() == QEvent::Show)) {
        positionJumpButton();
        if (m_followLatest) scheduleFollow();
        // Chips cannot wrap; size them again once the pane settles at another width.
        // Only a chip crossing the half-pane limit changes the journal: re-rendering
        // otherwise shifts the transcript after a panel toggle. A render's own
        // scrollbar changes are measured against the previous width, so skip them.
        const int width = m_browser->viewport()->width();
        const auto text = [](qreal chip, int pane) { return pane > 0 && chip > pane / 2.0; };
        if (!m_rendering && !m_html.isEmpty() && qAbs(width - m_chipWidth) > m_chipWidth / 10
            && std::any_of(m_chipWidths.cbegin(), m_chipWidths.cend(), [&](qreal chip) { return text(chip, width) != text(chip, m_chipWidth); }))
            m_relayout->start();
    }
    return QWidget::eventFilter(watched, event);
}

void ActivityView::activateLink(const QUrl &url)
{
    if (url.scheme() == "hgs-activity" && m_toggleKeys.contains(url.path())) {
        const auto key = url.path();
        m_expanded.insert(key, !m_expanded.value(key));
        // Expanding a card is an explicit reading action, even at the bottom.
        m_followLatest = false; render(false); updateJumpButton();
    } else if (url.scheme() == "hgs-file" && m_fileLinks.contains(url.path())) {
        emit fileReferenceActivated(m_fileLinks.value(url.path()));
    } else if(url.scheme()=="hgs-attachment" && m_attachmentLinks.contains(url.path())) {
        emit attachmentActivated(m_attachmentLinks.value(url.path()));
    } else if(url.scheme()=="hgs-message" && m_messageActions.contains(url.path())) {
        const auto action=m_messageActions.value(url.path());emit messageActionRequested(action.first,action.second);
    } else if(url.scheme()=="hgs-process" && m_processLinks.contains(url.path())) emit processRequested(url.path());
    else if (webLink(url)) emit externalLinkActivated(url);
}

void ActivityView::setAttachmentPreview(const QString &key, const QImage &image)
{
    if (image.isNull() || !m_previewFiles.contains(key) || !m_attachmentLinks.contains(key)) return;
    QImage tile(192, 128, QImage::Format_ARGB32_Premultiplied); tile.fill(Qt::transparent);
    const auto scaled = image.scaled(tile.size(), Qt::KeepAspectRatio, Qt::SmoothTransformation);
    QPainter painter(&tile); painter.drawImage((tile.width()-scaled.width())/2, (tile.height()-scaled.height())/2, scaled); painter.end();
    m_browser->document()->addResource(QTextDocument::ImageResource, QUrl("hgs-thumbnail:"+key), tile);
    // The reserved image dimensions stay constant. No setHtml, scroll reset or
    // selection change is needed when a remote thumbnail arrives.
    m_browser->document()->markContentsDirty(0, m_browser->document()->characterCount());
    m_browser->viewport()->update();
}

void ActivityView::render(bool contentUpdate)
{
    m_fileLinks.clear();
    m_attachmentLinks.clear();
    m_messageActions.clear();
    const bool searching = !m_searchResult.isEmpty();
    const QJsonObject details = searching ? QJsonObject() : m_details;
    const auto queue = details["input_queue"].toObject();
    m_queue->setVisible(m_tracked && !searching && !queue["id"].toString().isEmpty());
    if (m_queueText->text() != queue["text"].toString()) m_queueText->setText(queue["text"].toString());
    m_queueText->setToolTip(queue["text"].toString());
    m_queueSend->setEnabled(queue["can_send_now"].toBool() && !details["queue_sending"].toBool());
    m_queueSend->setText(details["queue_sending"].toBool() ? tr("Sending…") : tr("Send now"));
    m_queueSend->setToolTip(queue["hint"].toString());
    // Live state is separate from journal HTML: its animation and polls must
    // not rebuild the transcript or disturb a text selection.
    m_compaction->setVisible(m_tracked && !searching && details.value("phase") == "compacting"
        && details.value("runtime_state") == "live" && details.value("process_state") == "running"
        && details.value("state") != "archived");
    QJsonArray sourceEvents = searching ? QJsonArray{m_searchResult} : m_events;
    const QJsonArray localMessages = searching ? QJsonArray() : m_localMessages;
    for (const auto &value : localMessages) {
        const auto message = value.toObject();
        if (!message["message_id"].toString().isEmpty())
            m_deliveryKeys.insert(message["message_id"].toString(), "local-" + message["id"].toString());
    }
    for (int i = 0; i < sourceEvents.size(); ++i) {
        auto event = sourceEvents[i].toObject();
        const auto id = event.value("attachment_request_id").toString(event.value("message_id").toString());
        if (m_deliveryKeys.contains(id)) { event["activity_key"] = m_deliveryKeys.value(id); sourceEvents[i] = event; }
    }
    for(const auto &value:localMessages) {
        auto message=value.toObject();message["type"]="LocalMessage";message["detail"]=message.value("text");message["at"]=message.value("submitted_at");
        // The native hook can arrive before the durable delivery metadata.
        // Keep the cached file attached to that event, not as a second message.
        int match=-1;double nearest=10.;
        const auto submitted=message.value("submitted_text").toString(message.value("text").toString()).trimmed();
        if(message.value("status")=="sent")for(int i=0;i<sourceEvents.size();++i) {
            const auto event=sourceEvents[i].toObject();
            if(!event.value("agent_id").toString().isEmpty() || !QStringList{"UserPromptSubmit","UserPromptQueued","TurnStarted"}.contains(event.value("type").toString()))continue;
            const auto body=event.value("detail").toString().trimmed();
            const double distance=qAbs(event.value("at").toDouble()-message.value("at").toDouble());
            if(event.value("attachments").toArray().isEmpty() && distance<=nearest && !submitted.isEmpty() && !body.isEmpty() && (body.startsWith(submitted.left(1000)) || submitted.startsWith(body.left(1000)))){match=i;nearest=distance;}
        }
        if(match>=0){auto event=sourceEvents[match].toObject();event["activity_key"]=eventKey(message);event["detail"]=message.value("detail");event["attachments"]=message.value("attachments");event["attachment_submitted_text"]=message.value("submitted_text");sourceEvents[match]=event;continue;}
        sourceEvents.append(message);
    }
    const QString fallbackPrompt = searching ? QString() : m_fallbackPrompt;
    const QString fg = m_dark ? "#e8edf4" : "#1a2733";
    const QString muted = m_dark ? "#9eabba" : "#657487";
    const QString accent = m_dark ? "#8bdfc0" : "#167357";
    const QString surface = m_dark ? "#242d36" : "#f3f6f8";
    const QString userSurface = m_dark ? "#243730" : "#edf6f1";
    const QString codeSurface = m_dark ? "#171e25" : "#e8eef2";
    const QString warning = m_dark ? "#edbd77" : "#9b6216";
    const QString blue = m_dark ? "#98bdeb" : "#366ba9";
    const QString violet = m_dark ? "#c5a8f5" : "#6c43b8";
    const QString noticeSurface = m_dark ? "#2a2536" : "#f4f0fb";
    // Thinking reads like a reply but stays quieter: no card fill, a pale bar
    // and dimmer text at the same size.
    const QString thinkingBar = m_dark ? "#4f6680" : "#aec3dc";
    const QString thinkingText = m_dark ? "#b9c2cd" : "#424a53";
    const QString thinkingLabel = m_dark ? "#8ea3bd" : "#4f6b8f";
    const QString border = m_dark ? "#34404a" : "#dbe3e9";
    // GitHub Markdown on the Zerus card and page colours. Code blocks and zebra
    // rows keep the journal's code surface so that they stay visible on the card.
    auto agentTheme = MarkdownTheme::github(m_dark, QColor(surface), m_scale);
    auto pageTheme = MarkdownTheme::github(m_dark, QColor(m_dark ? "#1c2229" : "#ffffff"), m_scale);
    agentTheme.subtle = pageTheme.subtle = QColor(codeSurface);
    // Dark: GitHub's zebra and border steps relative to the grey card instead of near-black
    // code rows, and denser inline code so that it stands out from the card.
    agentTheme.stripe = pageTheme.stripe = QColor(m_dark ? "#2a333d" : codeSurface);
    if (m_dark) {
        agentTheme.border = pageTheme.border = QColor("#3a444e");
        agentTheme.chip.setAlphaF(0.33f); pageTheme.chip.setAlphaF(0.33f);
    }
    QString html = QString("<html><head><style>body{color:%1;font-size:13px;}p{margin:6px 0;line-height:135%;}h1,h2,h3,h4{font-size:14px;margin:10px 0 6px;}pre{white-space:pre-wrap;}a{color:%2;text-decoration:none;}li{margin-bottom:4px;}</style></head><body>").arg(fg, accent);
    html += QString("<p style='font-size:10px;color:%1;margin-bottom:12px;'>%2</p>").arg(muted, searching ? tr("SEARCH RESULT — Saved message") : tr("RECORDED ACTIVITY — Messages and tool excerpts"));
    if (details.value("history_truncated").toBool())
        html += QString("<p style='color:%1;font-size:11px;'>%2</p>").arg(muted, tr("Older events have expired from the journal."));
    if (searching && m_searchResult.value("content_truncated").toBool())
        html += QString("<p style='color:%1;font-size:11px;'>%2</p>").arg(muted, tr("Excerpt around the match. This message is too long to display in full."));

    const auto attachmentCards=[&](const QJsonArray &files,const QString &owner){
        QString result;
        for(int i=0;i<files.size();++i) {
            const auto file=files[i].toObject();const auto name=file.value("name").toString();
            // Location metadata changes when a local submission becomes a
            // durable receipt. Keep its loaded thumbnail and card identity.
            const QJsonObject identity{{"name",file["name"]},{"mime",file["mime"]},{"bytes",file["bytes"]},{"reference",file["reference"]}};
            const auto key=QString::fromLatin1(QCryptographicHash::hash((m_sessionKey+'\n'+m_conversation+'\n'+owner+':'+QString::number(i)).toUtf8()+QJsonDocument(identity).toJson(QJsonDocument::Compact),QCryptographicHash::Sha256).toHex());
            const bool available=!file.value("request_id").toString().isEmpty() || !file.value("local_path").toString().isEmpty();
            if(available)m_attachmentLinks.insert(key,file);
            const auto size=file.value("bytes").toInteger();const auto meta=size>0?tr("%1 KiB").arg(qMax<qint64>(1,(size+1023)/1024)):tr("File");
            const auto reference=file.value("reference").toString();
            const auto caption=escaped((reference.isEmpty()?QString():reference+" ")+name)+(available?QString(" &nbsp; <span style='color:%1'>%2 &nbsp; %3</span>").arg(muted,escaped(meta),tr("Open →")):QString());
            QString preview;
            if(available && file.value("mime").toString().startsWith("image/") && file.value("mime")!="image/svg+xml") {
                if(!m_previewFiles.contains(key)) {
                    m_previewFiles.insert(key,file);
                    QImage placeholder(192,128,QImage::Format_ARGB32_Premultiplied);placeholder.fill(Qt::transparent);
                    m_browser->document()->addResource(QTextDocument::ImageResource,QUrl("hgs-thumbnail:"+key),placeholder);
                    QTimer::singleShot(0,this,[this,key] {
                        if(m_attachmentLinks.contains(key))emit attachmentPreviewRequested(key,m_attachmentLinks.value(key));
                    });
                }
                preview=QString("<td width='96' height='64'><a href='hgs-attachment:%1'><img src='hgs-thumbnail:%1' width='96' height='64'></a></td>").arg(key);
            }
            result+=QString("<table cellspacing='0' cellpadding='8' style='margin-top:8px;border:1px solid %1;'><tr>%2<td>%3</td></tr></table>").arg(border,preview,
                available?QString("<a href='hgs-attachment:%1' style='color:%2;'>%3</a>").arg(key,accent,caption):tr("Attachment: %1").arg(escaped(name)));
        }
        return result;
    };
    const auto card = [&](const QString &key, const QString &label, const QString &stamp, const QString &body, bool user, bool notice = false, bool thinking = false) {
        auto content = body;
        // User text always starts with a paragraph. Anchor inside that block:
        // Qt discards an empty anchor between paragraphs, and a header-relative
        // offset shifts when Sending changes to Submitted or Sent.
        if (user) content.insert(content.indexOf('>') + 1, QString("<a name='item-body-%1'></a>").arg(escaped(key)));
        return QString("<table width='100%' cellspacing='0' cellpadding='0'><tr><td width='3' bgcolor='%1'></td><td bgcolor='%2' style='padding:11px 13px;'>"
            "<p style='font-size:11px;margin-top:0;margin-bottom:8px;'><a name='item-%3'></a><b style='color:%8;'>%4</b><span style='color:%5;'>%6</span></p>%7</td></tr></table><p style='font-size:5px;margin:0;'>&nbsp;</p>")
            .arg(thinking ? thinkingBar : notice ? violet : user ? accent : blue,
                 thinking ? pageTheme.canvas.name() : notice ? noticeSurface : user ? userSurface : surface, escaped(key), escaped(label), muted,
                stamp.isEmpty() ? QString() : QStringLiteral(" &nbsp;&nbsp; ") + escaped(stamp), content,
                 thinking ? thinkingLabel : notice ? violet : user ? accent : blue);
    };

    QList<QJsonObject> events;
    QSet<QString> seen;
    for (const auto &value : sourceEvents) {
        if (!value.isObject()) continue;
        const auto event = value.toObject(); const auto key = eventKey(event);
        // Kimi emits this lifecycle hook without text before TurnStarted.
        // It still drives session state, but adds no content to the transcript.
        if (event.value("type") == "UserPromptSubmit" && event.value("agent_id").toString().isEmpty()
            && event.value("detail").toString().trimmed().isEmpty()
            && event.value("attachments").toArray().isEmpty()) continue;
        // Claude reminds a minute after every finished turn that it waits for input
        // (an empty Notification, or idle_prompt); approvals and questions have their own events.
        if (event.value("type") == "Notification" && (event.value("detail").toString().trimmed().isEmpty()
            || event.value("notification_type") == "idle_prompt")) continue;
        if (!seen.contains(key)) { seen.insert(key); events.append(event); }
    }
    const bool sequenced = std::all_of(events.cbegin(), events.cend(), [](const QJsonObject &e) { return e.value("seq").toInteger() > 0; });
    const bool timed = std::all_of(events.cbegin(), events.cend(), [](const QJsonObject &e) { return e.value("at").toDouble() > 0; });
    std::stable_sort(events.begin(), events.end(), [sequenced, timed](const QJsonObject &a, const QJsonObject &b) {
        // A confirmed answer may be journaled after the next tool hook, but
        // retains the provider's actual answer time. Sequence is the cursor
        // and a stable tie-breaker, not the occurrence time of such events.
        if (timed && a.value("at") != b.value("at")) return a.value("at").toDouble() < b.value("at").toDouble();
        if (sequenced) return a.value("seq").toInteger() < b.value("seq").toInteger();
        return a.value("at").toDouble() < b.value("at").toDouble();
    });
    // Some providers include the same prompt in both submission and turn-start
    // hooks. Merge only adjacent acknowledgements; a repeated later request is
    // still a distinct message. Retain the submission key for scroll anchors.
    for (int i = 1; i < events.size();) {
        const auto &previous = events[i - 1];
        const auto &current = events[i];
        const auto previousType = previous.value("type").toString();
        const auto previousText=previous.value("attachment_submitted_text").toString(previous.value("detail").toString()).trimmed();
        const auto currentText=current.value("attachment_submitted_text").toString(current.value("detail").toString()).trimmed();
        if ((previousType == "UserPromptSubmit" || previousType == "UserPromptQueued")
            && current.value("type") == "TurnStarted"
            && messageRole(previous) == "user" && messageRole(current) == "user"
            && (isExcerptOf(previousText, currentText)
                || isExcerptOf(currentText, previousText))) {
            if (!current.value("attachments").toArray().isEmpty()) {
                events[i - 1]["attachments"] = current.value("attachments");
                events[i - 1]["attachment_submitted_text"] = current.value("attachment_submitted_text");
                events[i - 1]["detail"] = current.value("detail");
            } else if (previous.value("attachments").toArray().isEmpty() && current.value("detail").toString().size() > previous.value("detail").toString().size())
                events[i - 1]["detail"] = current.value("detail");
            events.removeAt(i);
        } else ++i;
    }
    QString prompt = details.value("prompt").toString(fallbackPrompt);
    QString answer = details.value("last_message").toString();
    for (int i = events.size() - 1; i >= 0; --i) {
        const auto role = messageRole(events[i]);
        if (role == "user" && events[i].value("type") != "QuestionAnswered"
            && !prompt.isEmpty() && isExcerptOf(events[i].value("attachment_submitted_text").toString(events[i].value("detail").toString()).trimmed(), prompt.trimmed())) {
            if(events[i].value("attachments").toArray().isEmpty()) events[i]["detail"] = prompt;
            prompt.clear();
        } else if (role == "assistant" && !answer.isEmpty() && isExcerptOf(events[i].value("detail").toString(), answer)) {
            events[i]["detail"] = answer; answer.clear();
        }
    }
    // Snapshots without a matching event have no reliable position in the
    // timeline. Show them as preceding context, never as a new message after
    // the tools. Matched snapshots above stay at their actual event position.
    if (!prompt.isEmpty() || !answer.isEmpty()) {
        html += QString("<p style='font-size:10px;color:%1;margin:12px 0 8px;'>%2</p>")
            .arg(muted, tr("RECORDED CONTEXT — Outside the available timeline"));
        if (!prompt.isEmpty()) {
            const auto replies = questionReplies(prompt);
            html += card("prompt-snapshot", replies.isEmpty() ? tr("You (recorded request)") : tr("You (recorded answer)"), {},
                replies.isEmpty() ? userMessage(prompt) : questionReplyBody(replies, muted, accent, codeSurface, border), true);
        }
        if (!answer.isEmpty()) html += card("answer-snapshot", tr("Agent (recorded response)"), {}, markdown(answer, agentTheme, m_fileLinks), false);
    }
    m_toggleKeys.clear();m_processLinks.clear();
    for (int i = 0; i < events.size();) {
        const auto event = events[i]; const auto role = messageRole(event);
        if (event.value("type") == "AgentThinking") {
            const auto text = event.value("detail").toString();
            if (!text.trimmed().isEmpty()) {
                auto thinkingTheme = pageTheme; thinkingTheme.fg = QColor(thinkingText);
                html += card(eventKey(event), tr("Thinking"), timeText(event), markdown(text, thinkingTheme, m_fileLinks), false, false, true);
            }
            ++i; continue;
        }
        if (role == "notice") {
            auto body = QString("<p style='margin:0;'>%1</p>").arg(escaped(event.value("detail").toString()).replace('\n', "<br>"));
            // Another session's message is meant to be read in full. A
            // subagent's report stays one line, like Claude's terminal, until opened.
            const auto origin = event.value("origin").toString();
            const auto report = event.value("report").toString();
            if (origin == "peer_message") {
                auto noticeTheme = MarkdownTheme::github(m_dark, QColor(noticeSurface), m_scale);
                noticeTheme.subtle = QColor(codeSurface);
                body = markdown(report.isEmpty() ? event.value("detail").toString() : report, noticeTheme, m_fileLinks);
            } else if (!report.isEmpty()) {
                const QString key = "report-" + eventKey(event); m_toggleKeys.insert(key);
                const bool expanded = m_expanded.value(key);
                body += QString("<p style='font-size:11px;margin:6px 0 0;'><a href='hgs-activity:%1' style='color:%2;'>%3</a></p>")
                    .arg(key, violet, expanded ? tr("▾ Hide report") : tr("▸ Show report"));
                if (expanded) {
                    auto noticeTheme = MarkdownTheme::github(m_dark, QColor(noticeSurface), m_scale);
                    noticeTheme.subtle = QColor(codeSurface);
                    body += markdown(report, noticeTheme, m_fileLinks);
                }
            }
            const QString label = origin == "subagent_report" ? tr("Subagent report")
                : origin == "peer_message" ? tr("Message from %1").arg(event.value("sender").toString()) : tr("Background task");
            html += card(eventKey(event), label, timeText(event), body, false, true);
            ++i; continue;
        }
        if (!role.isEmpty()) {
            const auto text = event.value("detail").toString();
            const auto replies = role == "user" ? questionReplies(text) : QJsonArray();
            const QString label = role == "user" ? (event.value("type") == "UserPromptQueued" ? tr("You (queued)") :
                event.value("type") == "QuestionAnswered" || !replies.isEmpty() ? tr("You (answer)") : tr("You")) : tr("Agent");
            auto stamp=timeText(event);
            if (role == "user" && event["type"] != "LocalMessage") {
                bool accepted = event["type"] != "UserPromptQueued";
                if (!accepted) for (int j=i+1;j<events.size();++j) {
                    const auto next = events[j];
                    if ((next["type"]=="UserPromptSubmit" || next["type"]=="TurnStarted") && next["detail"]==event["detail"]) { accepted=true; break; }
                }
                stamp += "  " + (accepted ? tr("Sent") : tr("Queued at this time"));
            }
            auto body = role == "user" ? (replies.isEmpty() ? userMessage(text) : questionReplyBody(replies, muted, accent, codeSurface, border))
                                       : markdown(text, agentTheme, m_fileLinks);
            if(event.value("type")=="LocalMessage") {
                const auto state=event.value("status").toString();stamp=state=="sending"?tr("Sending…"):state=="error"?tr("Not sent"):tr("Submitted to terminal");
                if(state=="error" && event.value("uncertain").toBool())stamp=tr("Delivery not confirmed");
                if(state=="error")body+=QString("<p style='font-size:11px;color:%1;'>%2</p>").arg(warning,escaped(event.value("error").toString()));
            }
            body+=attachmentCards(event.value("attachments").toArray(),eventKey(event));
            if(event.value("type")=="LocalMessage" && event.value("status")=="error" && !event.value("id").toString().isEmpty()) {
                const auto button=[&](const QString &action,const QString &label) {
                    const auto key=QString::fromLatin1(QCryptographicHash::hash((m_sessionKey+':'+event.value("id").toString()+':'+action).toUtf8(),QCryptographicHash::Sha256).toHex());
                    m_messageActions.insert(key,{event.value("id").toString(),action});
                    return QString("<td bgcolor='%1' style='border:1px solid %2;'><a href='hgs-message:%3' style='color:%4;'>&nbsp;%5&nbsp;</a></td>")
                        .arg(codeSurface,border,key,accent,escaped(label));
                };
                body+=QString("<table cellspacing='6' cellpadding='4'><tr>%1%2</tr></table>")
                    .arg(event.value("uncertain").toBool()?button("inspect",details.value("native_ui_available").toBool()?tr("Check native UI"):tr("Check Terminal")):button("retry",tr("Retry")),button("delete",tr("Delete")));
            }
            html += card(eventKey(event), label, stamp, body, role == "user");
            ++i; continue;
        }
        // Collapse adjacent operational events together. Message and attention
        // boundaries always remain visible; tools from separate turns do not mix.
        const bool thinking=event.value("type")=="AgentThinking";
        QList<QJsonObject> group{event}; ++i;
        while (!thinking && i < events.size() && events[i].value("type")!="AgentThinking" && group.size() < 24 && messageRole(events[i]).isEmpty()
               && !attention(event) && !attention(events[i])) group.append(events[i++]);
        const QString key = "group-" + eventKey(event); m_toggleKeys.insert(key);
        if (!m_expanded.contains(key) && attention(event)) m_expanded.insert(key, true);
        const bool expanded = m_expanded.value(key);
        QStringList tools;
        int calls = 0;
        for (const auto &e : group) {
            const QString name = e.value("tool").toString();
            if (!name.isEmpty() && !tools.contains(name)) tools.append(name);
            if (e.value("type") == "PreToolUse") ++calls;
        }
        const QString callLabel = calls == 1 ? tr("1 tool call") : tr("%1 tool calls").arg(calls);
        QString title = thinking ? tr("Thinking") : group.size() == 1 ? eventTitle(event.value("type").toString()) :
            calls > 0 ? tr("%1, %2 events").arg(callLabel).arg(group.size()) : tr("%1 activity events").arg(group.size());
        if (!tools.isEmpty()) title += QStringLiteral(" / ") + tools.mid(0, 3).join(", ") + (tools.size() > 3 ? QStringLiteral(" …") : QString());
        const QString color = attention(event) ? warning : muted;
        const auto last = group.last();
        QString preview = last.value("detail").toString().simplified();
        if (preview.size() > 110) preview = preview.left(109) + QChar(0x2026);
        html += QString("<table width='100%' cellspacing='0' cellpadding='9' style='border:1px solid %1;'><tr><td><a name='item-%2'></a>"
            "<a href='hgs-activity:%3' style='color:%4;'><b>%5 %6</b></a><span style='font-size:10px;color:%7;'> &nbsp; %8</span>")
            .arg(border, escaped(key), key, color, expanded ? QStringLiteral("▾") : QStringLiteral("▸"), escaped(title), muted, escaped(timeText(last)));
        if (!expanded && !preview.isEmpty()) html += QString("<p style='color:%1;font-size:11px;margin:4px 0 0;'>%2</p>").arg(muted, escaped(preview));
        if(expanded && thinking)html+=markdown(event.value("detail").toString(),pageTheme,m_fileLinks);
        else if (expanded) for (const auto &e : group) {
            QString label = eventTitle(e.value("type").toString());
            if (!e.value("tool").toString().isEmpty()) label += QStringLiteral(" / ") + e.value("tool").toString();
            if (!e.value("agent_id").toString().isEmpty()) label += tr(" (subagent %1)").arg(e.value("agent_id").toString());
            html += QString("<p style='font-size:11px;color:%1;margin:9px 0 4px;'><b>%2</b> &nbsp; %3</p>").arg(muted, escaped(label), escaped(timeText(e)));
            const auto process=e.value("process_id").toString();
            bool recordedProcess=false;
            for(const auto &job:m_details.value("processes").toObject().value("items").toArray())if(job.toObject().value("id").toString()==process){recordedProcess=true;break;}
            if(ProcessSettings::enabled() && recordedProcess && QRegularExpression("^[a-f0-9]{32}$").match(process).hasMatch()){
                m_processLinks.insert(process);
                html+=QString("<a href='hgs-process:%1' style='color:%2;'>%3</a>").arg(process,accent,tr("View process"));
            }
            const auto detail = e.value("detail").toString();
            if (!detail.isEmpty()) html += QString("<p style='font-family:monospace;font-size:12px;background:%1;padding:7px;'>%2</p>")
                .arg(codeSurface, escaped(detail).replace('\n', "<br>"));
        }
        html += "</td></tr></table><p style='font-size:5px;margin:0;'>&nbsp;</p>";
    }
    const bool hasContent = !events.isEmpty() || !prompt.isEmpty() || !answer.isEmpty() || !localMessages.isEmpty();
    if (!hasContent) {
        const bool nativeSession = m_details.value("backend") == "dsh" && m_details.value("runtime_state") == "live" && !m_details.value("read_only").toBool();
        const bool signIn = nativeSession && m_details.value("auth_required").toBool();
        const bool nativeReady = nativeSession && m_details.value("activity") == "idle";
        html += QString("<p style='margin-top:18px;'><b>%1</b></p><p style='color:%2;'>%3</p>")
            .arg(signIn ? tr("No messages yet") : nativeReady ? tr("Ready for your first message") : tr("Activity will appear here"), muted,
                signIn ? tr("After signing in, send a message below to start this session.") :
                nativeReady ? tr("Send a message below. The agent's replies, tools and approvals will appear here.") :
                m_tracked ? tr("Messages, tools and approvals will appear as your agent works.") : tr("This session has no recorded activity. Open its terminal to see the agent."));
    }
    html += "</body></html>";
    html = ContentScale::html(html, m_scale);
    if (html == m_html) return;
    const bool follow = !searching && ((m_initial && hasContent) || (m_followLatest && !m_browser->textCursor().hasSelection()));
    if (contentUpdate && !m_initial && !follow) {
        QSet<QString> added = seen; added.subtract(m_knownEvents);
        m_unseen += qMax(added.size(), qsizetype(added.isEmpty() && seen == m_knownEvents ? 1 : 0));
    }
    m_knownEvents = seen; m_html = html;
    if (hasContent) m_initial = false;
    const auto anchors = bookmarks(m_browser->document());
    const auto cursor = m_browser->textCursor();
    const Position selectionStart(cursor.anchor(), anchors), selectionEnd(cursor.position(), anchors);
    const auto top = m_browser->cursorForPosition(QPoint(2, 2));
    const Position topPosition(top.position(), anchors);
    const int topY = m_browser->cursorRect(top).top();
    m_rendering = true;
    // A document replacement emits intermediate geometry/scroll updates.
    // Keep this transaction synchronous: returning to the event loop with
    // updates disabled lets a parent repaint erase Activity for a whole frame.
    const bool updatesEnabled = m_browser->updatesEnabled();
    m_browser->setUpdatesEnabled(false);
    static_cast<JournalDocument *>(m_browser->document())->pixelRatio = m_browser->devicePixelRatioF();
    // One edit block for the markup and its chips. Qt lays out a long document
    // lazily, and a second edit would move later paragraphs without their layout:
    // the spacers between cards would keep zero height.
    QTextCursor edit(m_browser->document()); edit.beginEditBlock();
    m_browser->setHtml(html);
    // Before bookmarks are read: offsets on both sides of a refresh count chips as one character.
    // A chip may take at most half the pane, so that it and its container still fit.
    m_chipWidth = m_browser->viewport()->width();
    m_chipWidths.clear();
    MarkdownObjects::convertChips(m_browser->document(), agentTheme, !searching, m_chipWidth / 2.0, &m_chipWidths);
    edit.endEditBlock();
    // QTextDocument lays out long tables lazily. Resolve the final scroll
    // range before restoring the viewport and allowing its next paint.
    auto *layout = m_browser->document()->documentLayout();
    // documentSize() finishes that layout but announces the final size only on
    // the layout's next timer tick. Until then the scroll range keeps the lazy
    // estimate, and following the latest message would paint one frame past
    // the end of the document.
    emit layout->documentSizeChanged(layout->documentSize());
    const auto updatedAnchors = bookmarks(m_browser->document());
    const int length = m_browser->document()->characterCount();
    QTextCursor restored(m_browser->document()); restored.setPosition(selectionStart.restored(updatedAnchors, length));
    restored.setPosition(selectionEnd.restored(updatedAnchors, length), QTextCursor::KeepAnchor);
    m_browser->setTextCursor(restored);
    if (!follow) {
        QTextCursor restoredTop(m_browser->document()); restoredTop.setPosition(topPosition.restored(updatedAnchors, length));
        auto *bar = m_browser->verticalScrollBar();
        bar->setValue(bar->value() + m_browser->cursorRect(restoredTop).top() - topY);
    }
    m_rendering = false; m_followLatest = follow;
    if (searching) {
        QList<QTextEdit::ExtraSelection> highlights; QTextCursor found(m_browser->document()), first;
        while (highlights.size() < 300 && !m_searchQuery.isEmpty()) {
            found = m_browser->document()->find(m_searchQuery, found);
            if (found.isNull()) break;
            if (first.isNull()) first = found;
            QTextEdit::ExtraSelection h; h.cursor = found; h.format.setBackground(QColor(m_dark ? "#605032" : "#ffebb4"));
            h.format.setForeground(QColor(m_dark ? "#ffdda0" : "#6e4b00")); highlights.append(h);
        }
        m_browser->setExtraSelections(highlights);
        if (!first.isNull()) { first.clearSelection(); m_browser->setTextCursor(first); m_browser->ensureCursorVisible(); }
        updateJumpButton();
    } else if (follow) { jumpToLatest(); scheduleFollow(); }
    else updateJumpButton();
    m_browser->setUpdatesEnabled(updatesEnabled);
}
