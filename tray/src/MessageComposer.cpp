#include "MessageComposer.h"
#include "ContentScale.h"
#include "WorkspaceIcons.h"
#include "WorkspaceStyle.h"

#include <QBuffer>
#include <QComboBox>
#include <QDropEvent>
#include <QDragLeaveEvent>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QHBoxLayout>
#include <QImageReader>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QToolTip>
#include <QLabel>
#include <QMimeData>
#include <QMimeDatabase>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QScreen>
#include <QSignalBlocker>
#include <QStyle>
#include <QStylePainter>
#include <QStyleOptionButton>
#include <QTextDocument>
#include <QUrl>
#include <QVBoxLayout>
#include <functional>

namespace {
constexpr qsizetype MaxTextBytes = HgsClient::MaximumMessageBytes;
constexpr qsizetype MaxAttachmentBytes = HgsClient::MaximumAttachmentBytes;
constexpr qsizetype MaxTotalBytes = HgsClient::MaximumAttachmentsBytes;
constexpr qsizetype MaxAttachments = HgsClient::MaximumAttachments;

// Paint the rounded surface explicitly: translucent top-level widgets skip
// Qt's automatic background fill, which otherwise leaves the popup unpainted.
class SettingsPopup : public QFrame {
public:
    explicit SettingsPopup(QWidget *parent) : QFrame(parent, Qt::Popup | Qt::FramelessWindowHint) {
        setAttribute(Qt::WA_TranslucentBackground);
    }
protected:
    void paintEvent(QPaintEvent *) override {
        QStyleOption option; option.initFrom(this);
        QPainter painter(this); style()->drawPrimitive(QStyle::PE_Widget, &option, &painter, this);
    }
};

// The model may be long, but the effort and disclosure arrow must remain
// readable when the inspector leaves only a narrow composer column.
class SettingsButton : public QPushButton {
public:
    explicit SettingsButton(QWidget *parent) : QPushButton(parent) {
        setMinimumWidth(78); setMaximumWidth(330);
        setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
    }
    QSize minimumSizeHint() const override { return QSize(78, 34); }
protected:
    void paintEvent(QPaintEvent *) override {
        QStylePainter painter(this); QStyleOptionButton option; initStyleOption(&option);
        const QString suffix = property("suffix").toString();
        const int available = qMax(0, width() - 22);
        option.text = fontMetrics().elidedText(property("modelLabel").toString(), Qt::ElideRight,
            qMax(0, available - fontMetrics().horizontalAdvance(suffix))) + suffix;
        painter.drawControl(QStyle::CE_PushButton, option);
    }
};

class ComposeEdit : public QPlainTextEdit {
public:
    explicit ComposeEdit(QWidget *parent) : QPlainTextEdit(parent) {}
    std::function<void()> submit;
    std::function<bool()> escape;
    std::function<void(const QString &)> attachFile;
    std::function<void(const QImage &)> attachImage;
protected:
    bool canInsertFromMimeData(const QMimeData *source) const override {
        return source->hasImage() || source->hasUrls() || QPlainTextEdit::canInsertFromMimeData(source);
    }
    void insertFromMimeData(const QMimeData *source) override {
        if (source->hasUrls()) {
            const auto urls = source->urls();
            bool allLocal = !urls.isEmpty();
            for (const auto &url : urls) allLocal = allLocal && url.isLocalFile();
            if (allLocal) { for (const auto &url : urls) attachFile(url.toLocalFile()); return; }
        }
        if (source->hasImage()) {
            const auto data = source->imageData();
            QImage image = qvariant_cast<QImage>(data);
            if (image.isNull() && data.canConvert<QPixmap>()) image = qvariant_cast<QPixmap>(data).toImage();
            if (!image.isNull()) { attachImage(image); return; }
        }
        if (source->hasText()) insertPlainText(source->text());
    }
    bool event(QEvent *event) override {
        if (event->type() == QEvent::ShortcutOverride && static_cast<QKeyEvent *>(event)->key() == Qt::Key_Escape) {
            event->accept(); return true;
        }
        return QPlainTextEdit::event(event);
    }
    void keyPressEvent(QKeyEvent *event) override {
        if (!m_composing && event->key() == Qt::Key_Escape) {
            // Like Escape in Terminal: stop a working turn, otherwise leave the field.
            if (!(escape && escape())) clearFocus();
            event->accept(); return;
        }
        if (!m_composing && (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter)
            && !event->modifiers().testFlag(Qt::ShiftModifier)) {
            submit(); event->accept(); return;
        }
        QPlainTextEdit::keyPressEvent(event);
    }
    void dropEvent(QDropEvent *event) override {
        const auto *mime = event->mimeData();
        if (mime->hasImage() || mime->hasUrls()) {
            // We handle attachment insertion ourselves, so finish the native
            // drag first. Otherwise Qt retains its drag-feedback cursor and
            // autoscroll timer; that cursor overrides the real caret's paint
            // position even after subsequent mouse/keyboard navigation.
            QDragLeaveEvent leave;
            QPlainTextEdit::dragLeaveEvent(&leave);
            // QTextControl otherwise selects its insertion range after a drop,
            // making the next insertion replace the newly added reference.
            setTextCursor(cursorForPosition(event->position().toPoint()));
            insertFromMimeData(mime);
            event->acceptProposedAction();
            return;
        }
        QPlainTextEdit::dropEvent(event);
    }
    void inputMethodEvent(QInputMethodEvent *event) override {
        m_composing = !event->preeditString().isEmpty();
        QPlainTextEdit::inputMethodEvent(event);
    }
private:
    bool m_composing = false;
};
}

MessageComposer::MessageComposer(QWidget *parent) : QWidget(parent)
{
    setObjectName("messageComposer");
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 8, 0, 0); layout->setSpacing(7);
    m_attachmentScroll = new QScrollArea; m_attachmentScroll->setObjectName("messageAttachments");
    m_attachmentScroll->setWidgetResizable(true); m_attachmentScroll->setFrameShape(QFrame::NoFrame);
    m_attachmentScroll->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_attachmentScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded); m_attachmentScroll->setFixedHeight(78);
    m_attachmentList = new QWidget; m_attachmentsLayout = new QHBoxLayout(m_attachmentList);
    m_attachmentsLayout->setContentsMargins(0, 0, 0, 0); m_attachmentsLayout->setSpacing(8);
    m_attachmentScroll->setWidget(m_attachmentList); m_attachmentScroll->hide(); layout->addWidget(m_attachmentScroll);
    auto *edit = new ComposeEdit(this); m_editor = edit; edit->setObjectName("messageInput");
    edit->setAccessibleName(tr("Message to selected agent")); edit->setPlaceholderText(tr("Message this agent…"));
    edit->setFixedHeight(76); edit->setTabChangesFocus(true);
    layout->addWidget(edit);
    auto *actions = new QHBoxLayout; m_actions = actions; actions->setContentsMargins(0, 0, 0, 0); actions->setSpacing(8);
    m_attach = new QPushButton; m_attach->setObjectName("attachMessageFile");
    m_attach->setFixedSize(34, 34); m_attach->setIconSize(QSize(19, 19));
    m_attach->setToolTip(tr("Attach files, paste an image, or drop files onto this session"));
    m_attach->setAccessibleName(tr("Attach files to message"));
    m_status = new QLabel; m_status->setObjectName("messageStatus"); m_status->setWordWrap(true);
    m_status->setTextFormat(Qt::PlainText); m_status->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    m_retry = new QPushButton(tr("Allow retry")); m_retry->setObjectName("allowMessageRetry"); m_retry->hide();
    m_retry->setToolTip(tr("Check the terminal before sending again to avoid a duplicate message"));
    m_send = new QPushButton(tr("Send ↑")); m_send->setObjectName("sendMessage");
    m_send->setToolTip(tr("Send: Enter; new line: Shift+Enter"));
    m_stop = new QPushButton(tr("Stop")); m_stop->setObjectName("interruptAgent"); m_stop->hide();
    connect(m_stop,&QPushButton::clicked,this,[this]{emit interruptRequested(m_key);});
    m_settings = new SettingsButton(this); m_settings->setObjectName("sessionModelSettings"); m_settings->setFixedHeight(34);
    m_settings->setAccessibleName(tr("Model and reasoning effort for this session"));
    m_settingsPopup = new SettingsPopup(this); m_settingsPopup->setObjectName("sessionSettingsPopup");
    auto *settingsLayout = new QVBoxLayout(m_settingsPopup); settingsLayout->setContentsMargins(14, 12, 14, 12); settingsLayout->setSpacing(8);
    auto *modelLabel = m_modelLabel = new QLabel(tr("Model"));
    m_models = new QComboBox; m_models->setObjectName("sessionModelChoice"); m_models->setAccessibleName(tr("Model"));
    m_models->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon); m_models->setMinimumContentsLength(12);
    modelLabel->setBuddy(m_models); settingsLayout->addWidget(modelLabel); settingsLayout->addWidget(m_models);
    auto *effortLabel = m_effortLabel = new QLabel(tr("Reasoning effort"));
    m_efforts = new QComboBox; m_efforts->setObjectName("sessionEffortChoice"); m_efforts->setAccessibleName(tr("Reasoning effort"));
    effortLabel->setBuddy(m_efforts); settingsLayout->addWidget(effortLabel); settingsLayout->addWidget(m_efforts);
    m_settingsHint = new QLabel; m_settingsHint->setObjectName("sessionSettingsHint"); m_settingsHint->setWordWrap(true); m_settingsHint->setTextFormat(Qt::PlainText);
    settingsLayout->addWidget(m_settingsHint);
    auto *settingsActions = new QHBoxLayout; settingsActions->setSpacing(8); settingsActions->addStretch();
    auto *cancelSettings = new QPushButton(tr("Cancel")); cancelSettings->setObjectName("cancelSessionSettings");
    cancelSettings->setToolTip(tr("Close without applying changes"));
    m_applySettings = new QPushButton(tr("Apply")); m_applySettings->setObjectName("applySessionSettings");
    m_settingsTerminal = new QPushButton(tr("Open Terminal")); m_settingsTerminal->setObjectName("sessionSettingsTerminal"); m_settingsTerminal->hide();
    connect(m_settingsTerminal, &QPushButton::clicked, this, [this] {
        if (m_popupKey != m_key || !m_settingsTerminalAvailable) return;
        m_settingsPopup->hide(); emit settingsTerminalRequested(m_key);
    });
    settingsActions->addWidget(m_settingsTerminal);
    settingsActions->addWidget(cancelSettings); settingsActions->addWidget(m_applySettings); settingsLayout->addLayout(settingsActions);
    connect(cancelSettings, &QPushButton::clicked, m_settingsPopup, &QWidget::hide);
    connect(m_settings, &QPushButton::clicked, this, &MessageComposer::openSettings);
    connect(m_models, &QComboBox::currentIndexChanged, this, &MessageComposer::updateSettingsEfforts);
    connect(m_efforts, &QComboBox::currentIndexChanged, this, &MessageComposer::updateSettingsApply);
    connect(m_applySettings, &QPushButton::clicked, this, [this] {
        if (m_popupKey != m_key || !m_applySettings->isEnabled()) return;
        const QString model = m_models->currentData().toString(), effort = m_efforts->currentData().toString();
        m_settingsPopup->hide(); emit settingsRequested(m_key, model, effort);
    });
    actions->addWidget(m_attach); actions->addStretch(0); actions->addWidget(m_status, 1); actions->addWidget(m_retry); actions->addWidget(m_settings); actions->addWidget(m_stop); actions->addWidget(m_send);
    layout->addLayout(actions);
    m_feedback = new QHBoxLayout; m_feedback->setContentsMargins(0, 0, 0, 0); m_feedback->setSpacing(8); layout->addLayout(m_feedback);
    connect(edit, &QPlainTextEdit::textChanged, this, [this]() {
        if (m_loading || m_key.isEmpty()) return;
        auto &draft = m_drafts[m_key]; draft.text = m_editor->toPlainText();
        if (!draft.error && !draft.uncertain) draft.notice.clear();
        updateControls();
    });
    auto rememberCursor = [this] {
        if (m_loading || m_key.isEmpty() || m_drafts[m_key].sending) return;
        auto &draft = m_drafts[m_key];
        draft.position = m_editor->textCursor().position(); draft.anchor = m_editor->textCursor().anchor();
    };
    connect(edit, &QPlainTextEdit::cursorPositionChanged, this, rememberCursor);
    connect(edit, &QPlainTextEdit::selectionChanged, this, rememberCursor);
    edit->submit = [this]() { send(); };
    edit->escape = [this]() { return requestInterrupt(); };
    edit->attachFile = [this](const QString &path) { attachFile(path); };
    edit->attachImage = [this](const QImage &image) {
        if (qint64(image.width()) * image.height() > 32000000) { showError(tr("Image is too large. Use an image below 32 megapixels.")); return; }
        QByteArray data; QBuffer buffer(&data); buffer.open(QIODevice::WriteOnly);
        if (!image.save(&buffer, "PNG")) { showError(tr("Could not read the pasted image.")); return; }
        addAttachment(QStringLiteral("pasted-image.png"), QStringLiteral("image/png"), data);
    };
    connect(m_attach, &QPushButton::clicked, this, &MessageComposer::attachFiles);
    connect(m_send, &QPushButton::clicked, this, &MessageComposer::send);
    connect(m_retry, &QPushButton::clicked, this, [this]() {
        if (m_key.isEmpty()) return;
        auto &draft = m_drafts[m_key]; draft.uncertain = false; draft.notice.clear(); draft.error = false;
        updateControls(); m_editor->setFocus();
    });
    setTheme(true); updateControls();
}

void MessageComposer::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
    const bool narrow = width() < 640;
    if (narrow == m_narrow) return;
    m_narrow = narrow;
    if (narrow) {
        m_actions->setStretch(1, 1);
        m_actions->removeWidget(m_status);
        m_actions->removeWidget(m_retry);
        m_feedback->addWidget(m_status, 1); m_feedback->addWidget(m_retry);
    } else {
        m_actions->setStretch(1, 0);
        m_feedback->removeWidget(m_status); m_feedback->removeWidget(m_retry);
        m_actions->insertWidget(2, m_status, 1);
        m_actions->insertWidget(3, m_retry);
    }
}

void MessageComposer::setSessionKey(const QString &key)
{
    if (key == m_key) return;
    m_settingsPopup->hide();
    m_key = key; m_loading = true;
    restoreDraft();
    m_loading = false; rebuildAttachments(); updateControls();
}

void MessageComposer::restoreDraft()
{
    const auto draft = m_drafts.value(m_key);
    m_editor->setPlainText(draft.sending ? QString() : draft.text);
    auto cursor = m_editor->textCursor();
    const int length = m_editor->toPlainText().size();
    cursor.setPosition(qBound(0, draft.anchor, length));
    cursor.setPosition(qBound(0, draft.position, length), QTextCursor::KeepAnchor);
    m_editor->setTextCursor(cursor);
}

void MessageComposer::setInterruptAvailability(bool working, bool enabled, const QString &reason)
{
    m_stop->setVisible(working); m_stop->setEnabled(enabled);
    m_stop->setToolTip(reason.isEmpty()?tr("Interrupt the current turn, like Escape in Terminal. The session stays open; queued messages follow the agent's native behavior."):reason);
}

bool MessageComposer::requestInterrupt()
{
    if (m_key.isEmpty() || m_stop->isHidden() || !m_stop->isEnabled()) return false;
    emit interruptRequested(m_key); return true;
}

bool MessageComposer::offerDraft(const QString &key, const QString &text)
{
    if (key.isEmpty() || text.trimmed().isEmpty()) return false;
    auto &draft = m_drafts[key];
    // A draft the user started, or a message on its way, always wins.
    if (!draft.text.trimmed().isEmpty() || !draft.attachments.isEmpty() || draft.sending) return false;
    draft.text = text; draft.position = draft.anchor = int(text.size()); draft.notice.clear();
    if (key == m_key) { m_loading = true; restoreDraft(); m_loading = false; }
    updateControls(); return true;
}

void MessageComposer::setAvailability(bool available, const QString &reason)
{
    if (available != m_available || reason != m_unavailableReason) QToolTip::hideText();
    m_available = available; m_unavailableReason = reason; updateControls();
}

bool MessageComposer::isSending(const QString &key) const { return m_drafts.value(key).sending; }

bool MessageComposer::draftMatches(const QString &key, const QString &text, const QList<MessageAttachment> &attachments) const
{
    const auto draft=m_drafts.value(key);
    if(draft.text!=text || draft.attachments.size()!=attachments.size())return false;
    for(int i=0;i<attachments.size();++i)if(draft.attachments[i].name!=attachments[i].name
        || draft.attachments[i].mime!=attachments[i].mime || draft.attachments[i].data!=attachments[i].data
        || draft.attachments[i].reference!=attachments[i].reference)return false;
    return true;
}

void MessageComposer::setSending(const QString &key, bool preserveDraft)
{
    if(preserveDraft)m_preservedDrafts.insert(key,m_drafts.value(key));
    auto &draft = m_drafts[key]; draft.sending = true; draft.error = false; draft.notice = tr("Sending…");
    // Keep the submitted payload for failure recovery, but remove it from the
    // composer as soon as the owner accepts delivery and adds it to Activity.
    if (key == m_key) {
        m_loading = true; m_editor->clear(); m_loading = false;
        rebuildAttachments(); updateControls();
    }
}

void MessageComposer::deliveryFinished(const QString &key, bool ok, const QString &detail, bool uncertain)
{
    if(m_preservedDrafts.contains(key)) {
        m_drafts[key]=m_preservedDrafts.take(key);
        if(key==m_key){m_loading=true;restoreDraft();m_loading=false;rebuildAttachments();updateControls();}
        return;
    }
    auto &draft = m_drafts[key]; const bool wasSending = draft.sending;
    draft.sending = false; draft.error = !ok; draft.uncertain = uncertain;
    draft.notice = ok ? tr("Submitted to terminal") : detail;
    if (ok) { draft.text.clear(); draft.attachments.clear(); draft.nextAttachmentNumber = 1; }
    if (key != m_key) return;
    if (ok || wasSending) { m_loading = true; restoreDraft(); m_loading = false; }
    rebuildAttachments(); updateControls();
}

void MessageComposer::renameDraft(const QString &oldKey, const QString &newKey)
{
    if (oldKey == newKey || !m_drafts.contains(oldKey)) return;
    m_drafts.insert(newKey, m_drafts.take(oldKey));
    if(m_preservedDrafts.contains(oldKey))m_preservedDrafts.insert(newKey,m_preservedDrafts.take(oldKey));
    if (m_key == oldKey) { m_key.clear(); setSessionKey(newKey); }
}

void MessageComposer::send()
{
    if (m_key.isEmpty()) return;
    if (!m_send->isEnabled()) {
        const auto draft = m_drafts.value(m_key);
        if (!draft.sending && (!draft.text.trimmed().isEmpty() || !draft.attachments.isEmpty()))
            QToolTip::showText(m_editor->mapToGlobal(QPoint(10, m_editor->height())), m_status->text().toHtmlEscaped(), m_editor, {}, 5000);
        return;
    }
    QToolTip::hideText();
    const auto draft = m_drafts.value(m_key);
    emit sendRequested(m_key, draft.text, draft.attachments);
}

void MessageComposer::attachFiles()
{
    if (m_key.isEmpty() || isSending(m_key)) return;
    const QString key = m_key;
    const auto files = QFileDialog::getOpenFileNames(this, tr("Attach files"), {}, tr("All files (*)"));
    if (key != m_key) return;
    for (const auto &file : files) attachFile(file);
}

void MessageComposer::attachDroppedFiles(const QStringList &paths)
{
    if(!canAttachFiles())return;
    auto cursor=m_editor->textCursor();cursor.clearSelection();m_editor->setTextCursor(cursor);
    for(const auto &path:paths)attachFile(path);
    m_editor->setFocus(Qt::OtherFocusReason);
}

void MessageComposer::attachFile(const QString &path)
{
    QFile file(path); const QFileInfo info(file);
    if (!info.isFile() || info.size() > MaxAttachmentBytes) { showError(tr("Choose a file of at most 10 MiB.")); return; }
    if (!file.open(QIODevice::ReadOnly)) { showError(tr("Cannot read %1: %2").arg(info.fileName(), file.errorString())); return; }
    const QByteArray bytes = file.read(MaxAttachmentBytes + 1);
    addAttachment(info.fileName(), QMimeDatabase().mimeTypeForData(bytes).name(), bytes);
}

bool MessageComposer::addAttachment(const QString &name, const QString &mime, const QByteArray &data)
{
    if (m_key.isEmpty() || isSending(m_key)) return false;
    auto &draft = m_drafts[m_key]; qsizetype total = data.size();
    for (const auto &attachment : draft.attachments) total += attachment.data.size();
    if (draft.attachments.size() >= MaxAttachments) { showError(tr("Attach up to 8 files per message.")); return false; }
    if (data.isEmpty()) { showError(tr("The attachment is empty.")); return false; }
    if (data.size() > MaxAttachmentBytes || total > MaxTotalBytes) { showError(tr("Attachments: 10 MiB per file, 20 MiB per message.")); return false; }
    QString reference;
    do {
        reference = QString("[%1 #%2]").arg(mime.startsWith("image/") ? "Image" : "File").arg(draft.nextAttachmentNumber++);
    } while (draft.text.contains(reference));
    draft.attachments.append(MessageAttachment{name, mime, data, reference});
    auto cursor = m_editor->textCursor();
    const auto text = m_editor->toPlainText();
    const bool leading = cursor.selectionStart() > 0 && !text[cursor.selectionStart() - 1].isSpace();
    const bool trailing = cursor.selectionEnd() == text.size() || !text[cursor.selectionEnd()].isSpace();
    cursor.insertText((leading ? " " : "") + reference + (trailing ? " " : ""));
    m_editor->setTextCursor(cursor);
    if (!draft.uncertain) { draft.notice.clear(); draft.error = false; }
    rebuildAttachments(); updateControls(); return true;
}

void MessageComposer::showError(const QString &text)
{
    if (m_key.isEmpty()) return;
    auto &draft = m_drafts[m_key]; draft.notice = text; draft.error = true; updateControls();
}

void MessageComposer::rebuildAttachments()
{
    while (auto *item = m_attachmentsLayout->takeAt(0)) { delete item->widget(); delete item; }
    const auto draft = m_drafts.value(m_key);
    m_attachmentScroll->setVisible(!draft.sending && !draft.attachments.isEmpty());
    if (draft.sending) return;
    for (int i = 0; i < draft.attachments.size(); ++i) {
        const auto &attachment = draft.attachments[i];
        auto *tile = new QWidget; tile->setObjectName("attachmentTile"); tile->setFixedSize(174, 58);
        auto *row = new QHBoxLayout(tile); row->setContentsMargins(7, 6, 5, 6); row->setSpacing(7);
        auto *image = new QLabel; image->setFixedSize(38, 38); image->setAlignment(Qt::AlignCenter);
        QBuffer source; source.setData(attachment.data); source.open(QIODevice::ReadOnly);
        QImageReader reader(&source);
        const QSize sourceSize = reader.size();
        if (sourceSize.isValid() && qint64(sourceSize.width()) * sourceSize.height() <= 32000000) {
            reader.setScaledSize(sourceSize.scaled(76, 76, Qt::KeepAspectRatio));
            const QImage thumbnail = reader.read();
            if (!thumbnail.isNull()) image->setPixmap(QPixmap::fromImage(thumbnail).scaled(38, 38, Qt::KeepAspectRatio, Qt::SmoothTransformation));
        }
        if (image->pixmap().isNull()) image->setPixmap(style()->standardIcon(QStyle::SP_FileIcon).pixmap(26, 26));
        auto *text = new QLabel; text->setTextFormat(Qt::PlainText); text->setWordWrap(true);
        text->setText(attachment.reference + '\n' + text->fontMetrics().elidedText(attachment.name, Qt::ElideMiddle, 94) + '\n' + tr("%1 KiB").arg(qMax<qsizetype>(1, (attachment.data.size() + 1023) / 1024)));
        text->setToolTip(attachment.name); text->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
        auto *remove = new QPushButton(QStringLiteral("×")); remove->setObjectName("removeAttachment"); remove->setFixedSize(20, 26);
        remove->setAccessibleName(tr("Remove attachment %1").arg(attachment.name)); remove->setEnabled(!draft.sending);
        connect(remove, &QPushButton::clicked, this, [this, i]() {
            auto &current = m_drafts[m_key]; if (current.sending || i >= current.attachments.size()) return;
            const auto reference = current.attachments.takeAt(i).reference;
            if (!reference.isEmpty()) {
                auto edit = m_editor->textCursor(); edit.beginEditBlock();
                auto match = m_editor->document()->find(reference);
                while (!match.isNull()) {
                    match.removeSelectedText();
                    match = m_editor->document()->find(reference, match);
                }
                edit.endEditBlock();
            }
            rebuildAttachments(); updateControls();
        });
        row->addWidget(image); row->addWidget(text, 1); row->addWidget(remove); m_attachmentsLayout->addWidget(tile);
    }
    m_attachmentsLayout->addStretch();
}

void MessageComposer::updateControls()
{
    const auto draft = m_drafts.value(m_key);
    const bool hasContent = !draft.text.trimmed().isEmpty() || !draft.attachments.isEmpty();
    const bool tooLong = draft.text.toUtf8().size() > MaxTextBytes;
    m_editor->setEnabled(!m_key.isEmpty() && !draft.sending);
    m_attach->setEnabled(!m_key.isEmpty() && !draft.sending);
    for (auto *button : m_attachmentList->findChildren<QPushButton *>("removeAttachment")) button->setEnabled(!draft.sending);
    m_send->setEnabled(!m_key.isEmpty() && m_available && hasContent && !tooLong && !draft.sending && !draft.uncertain);
    m_send->setText(draft.sending ? tr("Sending…") : tr("Send ↑"));
    m_retry->setVisible(draft.uncertain && !draft.sending);
    m_settings->setEnabled(!m_key.isEmpty() && !draft.sending);
    if (m_settingsPopup->isVisible()) updateSettingsApply();
    QString status = draft.notice;
    if (status.isEmpty()) status = tooLong ? tr("Message exceeds 64 KiB.") : !m_available ? m_unavailableReason : tr("Enter to send; Shift+Enter for a new line");
    m_status->setText(status); m_status->setToolTip(status);
    setWorkspaceStyle(m_status, QString("font-size:11px;color:%1;").arg(draft.error || tooLong ? (m_dark ? "#f4ab9b" : "#a13224") : (m_dark ? "#a2adbc" : "#627082")));
}

void MessageComposer::setModelSettings(const QString &model, const QString &effort, const QJsonArray &options,
                                     bool enabled, const QString &reason, const QString &pendingModel,
                                     const QString &pendingEffort, const QString &applyWhen)
{
    m_model = model; m_effort = effort; m_modelOptions = options; m_settingsEnabled = enabled;
    m_settingsReason = reason; m_pendingModel = pendingModel; m_pendingEffort = pendingEffort; m_applyWhen = applyWhen;
    updateSettingsButton();
    if (m_settingsPopup->isVisible()) updateSettingsApply();
}

void MessageComposer::updateSettingsButton()
{
    const bool pending = !m_pendingModel.isEmpty() || !m_pendingEffort.isEmpty();
    const QString model = m_model.isEmpty() ? tr("Model") : m_model;
    const QString suffix = (m_effort.isEmpty() ? QString() : "  " + m_effort) + (pending ? "  ◷" : QString()) + "  ⌄";
    m_settings->setProperty("modelLabel", model); m_settings->setProperty("suffix", suffix);
    m_settings->setText(model + suffix); m_settings->setAccessibleName(tr("Model and reasoning effort: %1").arg(model + suffix));
    QString hint = tr("Current: %1%2").arg(model, m_effort.isEmpty() ? QString() : "  " + m_effort);
    if (pending) hint += '\n' + tr("Pending: %1%2").arg(m_pendingModel.isEmpty() ? model : m_pendingModel,
        m_pendingEffort.isEmpty() ? QString() : "  " + m_pendingEffort);
    if (!m_settingsReason.isEmpty()) hint += '\n' + m_settingsReason;
    m_settings->setToolTip(hint); m_settings->updateGeometry(); m_settings->update();
}

void MessageComposer::openSettings()
{
    if (m_key.isEmpty()) return;
    m_popupKey = m_key;
    const QString model = m_pendingModel.isEmpty() ? m_model : m_pendingModel;
    const QSignalBlocker blocker(m_models); m_models->clear();
    for (const auto &value : m_modelOptions) {
        const auto option = value.toObject(); const auto id = option.value("id").toString();
        if (!id.isEmpty() && m_models->findData(id) < 0) m_models->addItem(option.value("label").toString(id), id);
    }
    if (m_models->findData(model) < 0 && !model.isEmpty()) m_models->insertItem(0, model, model);
    m_models->setCurrentIndex(m_models->findData(model));
    updateSettingsEfforts(); updateSettingsApply();
    const QRect available = screen()->availableGeometry();
    m_settingsPopup->setFixedWidth(qMin(360, available.width() - 24)); m_settingsPopup->adjustSize();
    QPoint position = m_settings->mapToGlobal(QPoint(m_settings->width() - m_settingsPopup->width(), -m_settingsPopup->height() - 7));
    position.setX(qBound(available.left() + 8, position.x(), available.right() - m_settingsPopup->width() - 8));
    position.setY(qBound(available.top() + 8, position.y(), available.bottom() - m_settingsPopup->height() - 8));
    m_settingsPopup->move(position); m_settingsPopup->show();
    (m_settingsTerminal->isVisible() ? static_cast<QWidget *>(m_settingsTerminal) : m_models)->setFocus(Qt::PopupFocusReason);
}

void MessageComposer::updateSettingsEfforts()
{
    const QString preferred = m_efforts->isVisible() && !m_efforts->currentData().toString().isEmpty()
        ? m_efforts->currentData().toString() : m_pendingEffort.isEmpty() ? m_effort : m_pendingEffort;
    const QSignalBlocker blocker(m_efforts); m_efforts->clear();
    for (const auto &value : m_modelOptions) {
        const auto option = value.toObject();
        if (option.value("id").toString() != m_models->currentData().toString()) continue;
        for (const auto &effort : option.value("effort_options").toArray()) {
            const QString id = effort.toString();
            if (!id.isEmpty()) m_efforts->addItem(id, id);
        }
        break;
    }
    if (m_efforts->count() == 0) m_efforts->addItem(tr("Provider default"), QString());
    const int selected = m_efforts->findData(preferred);
    m_efforts->setCurrentIndex(selected >= 0 ? selected : 0); updateSettingsApply();
}

void MessageComposer::setSettingsTerminalAvailable(bool available)
{
    m_settingsTerminalAvailable = available;
    if (m_settingsPopup->isVisible()) updateSettingsApply();
}

void MessageComposer::updateSettingsApply()
{
    const bool native = m_settingsTerminalAvailable && !m_settingsEnabled;
    m_settingsTerminal->setVisible(native);
    for (auto *widget : QList<QWidget *>{m_models,m_efforts,m_modelLabel,m_effortLabel,m_applySettings}) widget->setVisible(!native);
    bool offered = false;
    for (const auto &value : m_modelOptions) offered |= value.toObject().value("id").toString() == m_models->currentData().toString();
    const bool enabled = m_settingsEnabled && m_popupKey == m_key && !isSending(m_key);
    m_models->setEnabled(enabled && m_models->count() > 1);
    m_efforts->setEnabled(enabled && offered && !m_efforts->currentData().toString().isEmpty());
    const QString intendedModel = m_pendingModel.isEmpty() ? m_model : m_pendingModel;
    const bool pending = !m_pendingModel.isEmpty() || !m_pendingEffort.isEmpty();
    const QString intendedEffort = pending ? m_pendingEffort : m_effort;
    const bool changed = m_models->currentData().toString() != intendedModel || m_efforts->currentData().toString() != intendedEffort;
    m_applySettings->setEnabled(enabled && offered && (changed || (pending && m_applyWhen == "now")));
    m_applySettings->setText(m_applyWhen == "resume" ? tr("Save for resume") : m_applyWhen == "ready" ? tr("Save change") : tr("Apply"));
    QString hint = m_settingsReason;
    if (hint.isEmpty()) hint = !m_settingsEnabled ? tr("Model settings are not available for this session.")
        : m_applyWhen == "resume" ? tr("Saved for the next resume. The session stays stopped.")
        : m_applyWhen == "ready" ? tr("Applies when the agent is ready. Messages sent while it is working use the current model.")
        : tr("Applies to this session only.");
    if (pending)
        hint = tr("Pending: %1%2\nCurrent: %3%4\n%5").arg(intendedModel, intendedEffort.isEmpty() ? QString() : "  " + intendedEffort,
            m_model, m_effort.isEmpty() ? QString() : "  " + m_effort, hint);
    m_settingsHint->setText(hint);
}

void MessageComposer::setTheme(bool dark)
{
    m_dark = dark;
    updateSettingsButton();
    m_attach->setIcon(workspaceIcon("attachment", dark ? QColor("#c5cfdb") : QColor("#536477")));
    m_stop->setIcon(workspaceIcon("stop",dark ? QColor("#ff9ca8") : QColor("#b52d48")));
    setStyleSheet(QString(R"(
        QPlainTextEdit#messageInput { background:%1; color:%2; border:1px solid %3; border-radius:9px; padding:%8px; font-size:%9px; selection-background-color:%4; }
        QPlainTextEdit#messageInput:focus { border-color:%5; }
        QScrollArea#messageAttachments, QWidget#messageComposer { background:transparent; border:0; }
        QWidget#attachmentTile { background:%1; border:1px solid %3; border-radius:8px; }
        QWidget#attachmentTile QLabel { font-size:10px; border:0; }
        QPushButton#removeAttachment { padding:0; border:0; background:transparent; }
        QPushButton#sendMessage { background:%5; color:%6; border:1px solid %5; padding:8px 15px; border-radius:7px; font-weight:600; }
        QPushButton#sendMessage:disabled { background:%1; color:%7; border-color:%3; }
        QPushButton#sessionModelSettings { padding:0 10px; font-size:11px; min-height:0; border:1px solid transparent; border-radius:7px; color:%2; background:transparent; }
        QPushButton#sessionModelSettings:hover { background:%1; border-color:%3; }
        QPushButton#sessionModelSettings:focus[keyboardFocus="true"] { border-color:%5; }
        QFrame#sessionSettingsPopup { background:%1; color:%2; border:1px solid %3; border-radius:9px; }
        QFrame#sessionSettingsPopup QLabel { color:%2; border:0; }
        QLabel#sessionSettingsHint { color:%7; font-size:11px; }
        QFrame#sessionSettingsPopup QComboBox { background:%1; color:%2; border:1px solid %3; border-radius:6px; padding:7px 26px 7px 9px; }
        QFrame#sessionSettingsPopup QComboBox::drop-down { border:0; width:24px; }
        QFrame#sessionSettingsPopup QComboBox::down-arrow { image:url(:/hgs/chevron-down.svg); width:14px; height:14px; }
        QFrame#sessionSettingsPopup QComboBox QAbstractItemView { background:%1; color:%2; selection-background-color:%4; }
        QPushButton#applySessionSettings { padding:7px 12px; background:%5; color:%6; border:1px solid %5; border-radius:6px; }
        QPushButton#applySessionSettings:disabled { background:%1; color:%7; border-color:%3; }
        QPushButton#cancelSessionSettings { padding:7px 12px; background:transparent; color:%2; border:1px solid %3; border-radius:6px; }
        QPushButton#cancelSessionSettings:hover { background:%4; }
        QPushButton#cancelSessionSettings:focus[keyboardFocus="true"] { border-color:%5; }
        QPushButton#attachMessageFile { padding:0; border:1px solid %3; border-radius:7px; background:transparent; }
        QPushButton#attachMessageFile:hover { background:%1; border-color:%5; }
        QPushButton#attachMessageFile:focus[keyboardFocus="true"] { border-color:%5; }
        QPushButton#allowMessageRetry { padding:7px 10px; font-size:11px; }
    )").arg(dark ? "#171d24" : "#f8fafb", dark ? "#e8edf4" : "#1a2733", dark ? "#3d4855" : "#dce2e8",
             dark ? "#365b4d" : "#c7eadd", dark ? "#8bdfc0" : "#167357", dark ? "#10231b" : "#ffffff", dark ? "#85909e" : "#6d7784")
        .arg(ContentScale::px(9, m_scale)).arg(ContentScale::px(13, m_scale)) + workspaceScrollbars(dark));
    updateControls();
}

void MessageComposer::setContentScale(double scale)
{
    scale = ContentScale::clamp(scale);
    if (qFuzzyCompare(scale, m_scale)) return;
    m_scale = scale;
    // Keep the same number of visible lines as at the native size.
    m_editor->setFixedHeight(ContentScale::px(76, scale));
    setTheme(m_dark);
}
