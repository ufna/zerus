#include "QuestionCard.h"

#include <QAbstractButton>
#include <QApplication>
#include <QButtonGroup>
#include <QDateTime>
#include <QCheckBox>
#include <QFrame>
#include <QHBoxLayout>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QRadioButton>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QStackedWidget>
#include <QStyle>
#include <QStyleOption>
#include <QTabBar>
#include <QToolButton>
#include <QVBoxLayout>

namespace {
// Reserve the same space for every submit label, including the shorter busy
// label used while Skip is being delivered. Let Qt size it for the current
// font and style instead of fixing a pixel width.
class SubmitButton final : public QPushButton {
public:
    explicit SubmitButton(const QStringList &labels) : QPushButton(labels.first()), m_labels(labels)
    {
        setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
    }
    QSize sizeHint() const override
    {
        ensurePolished();
        QStyleOptionButton option; initStyleOption(&option);
        QSize size;
        for (const auto &label : m_labels) {
            option.text = label;
            size = size.expandedTo(style()->sizeFromContents(QStyle::CT_PushButton, &option,
                fontMetrics().size(Qt::TextShowMnemonic, label), this));
        }
        return size;
    }
    QSize minimumSizeHint() const override { return sizeHint(); }
private:
    QStringList m_labels;
};

QString customAnswerProblem(const QString &text)
{
    if (text.toUtf8().size() > 4096) return QObject::tr("Keep your custom answer under 4 KiB.");
    if (text.contains('\n') || text.contains('\r')) return QObject::tr("Use one line for a custom answer. Multiline input is available in Terminal.");
    for (const auto ch : text) if (ch.unicode() < 0x20 || (ch.unicode() >= 0x7f && ch.unicode() <= 0x9f))
        return QObject::tr("Remove control characters from your custom answer.");
    if (text.trimmed().startsWith('/') || text.trimmed().startsWith('!'))
        return QObject::tr("Use Terminal for answers starting with / or !.");
    return {};
}

class Choice final : public QFrame {
public:
    Choice(bool multi, const QString &label, const QString &description, QWidget *parent)
        : QFrame(parent), button(multi ? static_cast<QAbstractButton *>(new QCheckBox(this)) : new QRadioButton(this))
    {
        setObjectName("questionChoice"); setProperty("checked", false);
        auto *row = new QHBoxLayout(this); row->setContentsMargins(10, 9, 12, 9); row->setSpacing(8);
        button->setFixedWidth(20); button->setAccessibleName(label);
        button->setAccessibleDescription(description);
        row->addWidget(button, 0, Qt::AlignTop);
        auto *copy = new QVBoxLayout; copy->setSpacing(3); copy->setContentsMargins(0, 0, 0, 0);
        auto *title = new QLabel(label, this); title->setObjectName("questionChoiceTitle");
        title->setTextFormat(Qt::PlainText); title->setWordWrap(true); title->setAttribute(Qt::WA_TransparentForMouseEvents);
        title->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
        copy->addWidget(title);
        if (!description.isEmpty()) {
            auto *detail = new QLabel(description, this); detail->setObjectName("questionChoiceDescription");
            detail->setTextFormat(Qt::PlainText); detail->setWordWrap(true); detail->setAttribute(Qt::WA_TransparentForMouseEvents);
            detail->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
            copy->addWidget(detail);
        }
        row->addLayout(copy, 1);
        connect(button, &QAbstractButton::toggled, this, [this](bool checked) {
            setProperty("checked", checked); style()->unpolish(this); style()->polish(this); update();
        });
    }
    QAbstractButton *button;
protected:
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton && button->isEnabled()) {
            button->setFocus(Qt::MouseFocusReason); button->click(); event->accept(); return;
        }
        QFrame::mousePressEvent(event);
    }
};
}

QuestionCard::QuestionCard(QWidget *parent) : QWidget(parent)
{
    setObjectName("questionCard"); setAttribute(Qt::WA_StyledBackground);
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(14, 12, 14, 12); layout->setSpacing(9);
    auto *header = new QHBoxLayout; header->setContentsMargins(0, 0, 0, 0);
    m_heading = new QLabel(tr("Agent needs your answer")); m_heading->setObjectName("questionHeading");
    m_heading->setTextFormat(Qt::PlainText); header->addWidget(m_heading, 1);
    m_progress = new QLabel; m_progress->setObjectName("questionProgress"); header->addWidget(m_progress);
    layout->addLayout(header);
    auto *metadata=new QHBoxLayout;metadata->setContentsMargins(0,0,0,0);
    m_askedAt=new QLabel;m_askedAt->setObjectName("questionAskedAt");m_askedAt->setTextFormat(Qt::PlainText);
    m_pendingCount=new QLabel;m_pendingCount->setObjectName("questionPendingCount");m_pendingCount->setTextFormat(Qt::PlainText);
    m_pendingCount->setToolTip(tr("Questions waiting for an answer, including the questions shown in this card."));
    m_queuePrevious=new QToolButton;m_queuePrevious->setObjectName("questionQueuePrevious");m_queuePrevious->setArrowType(Qt::LeftArrow);
    m_queueNext=new QToolButton;m_queueNext->setObjectName("questionQueueNext");m_queueNext->setArrowType(Qt::RightArrow);
    m_queuePrevious->setAccessibleName(tr("Previous queued question"));m_queueNext->setAccessibleName(tr("Next queued question"));
    for(auto *button:{m_queuePrevious,m_queueNext}) {button->setFixedSize(26,24);button->hide();}
    connect(m_queuePrevious,&QToolButton::clicked,this,[this]{emit queueNavigationRequested(-1);});
    connect(m_queueNext,&QToolButton::clicked,this,[this]{emit queueNavigationRequested(1);});
    metadata->addWidget(m_askedAt);metadata->addStretch();metadata->addWidget(m_pendingCount);
    metadata->addWidget(m_queuePrevious);metadata->addWidget(m_queueNext);layout->addLayout(metadata);
    m_tabs = new QTabBar; m_tabs->setObjectName("questionTabs");
    m_tabs->setExpanding(false); m_tabs->setDrawBase(false); m_tabs->setUsesScrollButtons(true); m_tabs->setElideMode(Qt::ElideNone);
    layout->addWidget(m_tabs);
    m_pages = new QStackedWidget; m_pages->setObjectName("questionPages");
    m_pages->setMinimumHeight(155); m_pages->setMaximumHeight(330); layout->addWidget(m_pages, 1);
    m_status = new QLabel; m_status->setObjectName("questionStatus"); m_status->setWordWrap(true); m_status->setTextFormat(Qt::PlainText);
    m_status->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    layout->addWidget(m_status);
    auto *footer = new QHBoxLayout; footer->setContentsMargins(0, 0, 0, 0);
    m_terminal = new QPushButton(tr("Open Terminal")); m_terminal->setObjectName("questionOpenTerminal");
    m_retry = new QPushButton(tr("Allow retry")); m_retry->setObjectName("questionAllowRetry");
    m_retry->setToolTip(tr("Check the agent in Terminal before retrying to avoid submitting twice."));
    m_submit = new SubmitButton({tr("Submit answers"), tr("Submit answer"), tr("Submitting…")}); m_submit->setObjectName("submitQuestionAnswer");
    m_previous = new QPushButton(tr("← Back")); m_previous->setObjectName("questionPrevious");
    m_next = new QPushButton(tr("Next →")); m_next->setObjectName("questionNext");
    m_approve = new QPushButton(tr("Approve once (3)"));m_approve->setObjectName("approveToolRequest");
    m_deny = new QPushButton(tr("Deny"));m_deny->setObjectName("denyToolRequest");
    m_skip = new QPushButton(tr("Skip")); m_skip->setObjectName("skipQuestion");
    m_skip->setToolTip(tr("Dismiss this optional question without sending an answer. The agent can continue."));
    connect(m_skip,&QPushButton::clicked,this,[this]{
        if(!m_skip->isEnabled())return;
        QJsonArray result;for(const auto &form:m_forms)result.append(QJsonObject{{"question_id",form.id},{"skip",true}});
        emit answerRequested(m_session,m_id,result);
    });
    footer->addWidget(m_terminal); footer->addStretch(); footer->addWidget(m_skip); footer->addWidget(m_retry); footer->addWidget(m_previous); footer->addWidget(m_next); footer->addWidget(m_submit); layout->addLayout(footer);
    connect(m_previous,&QPushButton::clicked,this,[this]{m_tabs->setCurrentIndex(m_tabs->currentIndex()-1);});
    connect(m_next,&QPushButton::clicked,this,[this]{m_tabs->setCurrentIndex(m_tabs->currentIndex()+1);});
    footer->addWidget(m_deny);footer->addWidget(m_approve);
    for(auto *button:{m_approve,m_deny})connect(button,&QPushButton::clicked,this,[this,button]{
        if(!button->isEnabled()||!m_question.value("approval").toBool())return;
        const auto question=m_question.value("questions").toArray().first().toObject();
        emit answerRequested(m_session,m_id,QJsonArray{QJsonObject{{"question_id",question.value("id")},
            {"selected_option_ids",QJsonArray{button==m_approve?"allow":"deny"}},{"text",""}}});
    });
    m_reviewTimer.setInterval(100);connect(&m_reviewTimer,&QTimer::timeout,this,[this]{
        if(!m_question.value("approval").toBool())return;
        if(!isVisible()||!window()->isActiveWindow()||visibleRegion().isEmpty()||!m_available) {
            m_reviewClock.invalidate();m_reviewSeconds=3;
        } else {
            if(!m_reviewClock.isValid())m_reviewClock.start();
            m_reviewSeconds=qMax(0,3-int(m_reviewClock.elapsed()/1000));
        }
        updateControls();
    });m_reviewTimer.start();
    connect(m_terminal, &QPushButton::clicked, this, &QuestionCard::openTerminalRequested);
    connect(m_submit, &QPushButton::clicked, this, &QuestionCard::submit);
    connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
        m_pages->setCurrentIndex(index);
        if (!m_loading && !m_key.isEmpty()) m_drafts[m_key].page = index;
        updateControls();
    });
    connect(m_retry, &QPushButton::clicked, this, [this] {
        if (m_key.isEmpty()) return;
        auto &draft = m_drafts[m_key]; draft.uncertain = false; draft.error = false; draft.notice.clear(); updateControls();
    });
    setTheme(true); hide();
}

QString QuestionCard::identity(const QString &sessionKey, const QString &questionId, const QString &hash)
{
    return QString::fromUtf8(QJsonDocument(QJsonArray{sessionKey, questionId, hash}).toJson(QJsonDocument::Compact));
}

void QuestionCard::setQuestion(const QString &sessionKey, const QJsonObject &question, int pendingQuestions)
{
    const QString id = question.value("question_id").toString();
    const QString key = id.isEmpty() ? QString() : identity(sessionKey, id, question.value("question_hash").toString());
    const bool changed = key != m_key || question.value("questions") != m_question.value("questions");
    m_session = sessionKey; m_id = id; m_key = key; m_question = question;
    const double created=question.value("created_at").toDouble();
    const auto asked=created>0 && created<253402300800. ? QDateTime::fromSecsSinceEpoch(qint64(created)).toLocalTime() : QDateTime();
    m_askedAt->setVisible(asked.isValid());
    m_askedAt->setText(asked.isValid()?tr("Asked %1").arg(asked.toString(asked.date().year()==QDate::currentDate().year()?"d MMM HH:mm:ss":"d MMM yyyy HH:mm:ss")):QString());
    m_askedAt->setToolTip(asked.isValid()?asked.toString("d MMM yyyy HH:mm:ss t"):QString());
    const int count=qMax(pendingQuestions,question.value("questions").toArray().size());
    m_pendingCount->setText(tr("%1 pending").arg(count));m_pendingCount->setVisible(count>0);
    if (!key.isEmpty()) m_latestKeys.insert(identity(sessionKey, id), key);
    if (changed) {m_queueIndex=-1;m_queueCount=0;m_reviewClock.invalidate();m_reviewSeconds=3;rebuild();}
    updateControls(); setVisible(!m_key.isEmpty());
}

void QuestionCard::setAvailability(bool available, const QString &reason)
{
    m_available = available; m_unavailableReason = reason; updateControls();
}

void QuestionCard::setQueueNavigation(int index, int count)
{
    m_queueIndex=index;m_queueCount=count;
    const bool visible=m_question.value("optional").toBool() && count>1 && index>=0 && index<count;
    m_queuePrevious->setVisible(visible);m_queueNext->setVisible(visible);
    m_queuePrevious->setEnabled(visible && index>0);m_queueNext->setEnabled(visible && index+1<count);
    const QString position=tr("Request %1 of %2, newest first. Draft answers are kept when switching.").arg(index+1).arg(count);
    m_queuePrevious->setToolTip(tr("Previous queued question (newer)")+"\n"+position);
    m_queueNext->setToolTip(tr("Next queued question (older)")+"\n"+position);
    updateControls();
}

QString QuestionCard::callbackKey(const QString &sessionKey, const QString &questionId) const
{
    const QString pair = identity(sessionKey, questionId);
    return m_sendingKeys.value(pair, m_latestKeys.value(pair));
}

void QuestionCard::setSending(const QString &sessionKey, const QString &questionId, bool sending)
{
    const QString pair = identity(sessionKey, questionId), key = callbackKey(sessionKey, questionId);
    if (key.isEmpty()) return;
    auto &draft = m_drafts[key]; draft.sending = sending; draft.error = false;
    draft.notice = sending ? tr("Submitting your answer…") : QString();
    if (sending) m_sendingKeys.insert(pair, key); else m_sendingKeys.remove(pair);
    updateControls();
}

void QuestionCard::setError(const QString &sessionKey, const QString &questionId, const QString &detail, bool uncertain)
{
    const QString key = callbackKey(sessionKey, questionId);
    if (key.isEmpty()) return;
    auto &draft = m_drafts[key]; draft.sending = false; draft.error = true; draft.uncertain = uncertain;
    draft.notice = uncertain ? tr("Delivery is not confirmed. Check Terminal before retrying. %1").arg(detail) : detail;
    m_sendingKeys.remove(identity(sessionKey, questionId)); updateControls();
}

void QuestionCard::setAnswered(const QString &sessionKey, const QString &questionId)
{
    const QString key = callbackKey(sessionKey, questionId);
    if (key.isEmpty()) return;
    auto &draft = m_drafts[key]; draft.sending = false; draft.answered = true; draft.error = false; draft.uncertain = false;
    draft.notice = m_question.value("optional").toBool() ? tr("Response recorded.") : tr("Answer sent. Waiting for the agent…");
    m_sendingKeys.remove(identity(sessionKey, questionId)); updateControls();
}

void QuestionCard::rebuild()
{
    m_loading = true; m_forms.clear();
    while (m_pages->count()) { auto *page = m_pages->widget(0); m_pages->removeWidget(page); delete page; }
    while (m_tabs->count()) m_tabs->removeTab(0);
    const auto draft = m_drafts.value(m_key);
    int index = 0;
    for (const auto &value : m_question.value("questions").toArray()) {
        const auto question = value.toObject(); Form form;
        form.id = question.value("id").toString(); form.multi = question.value("multi_select").toBool();
        form.allowOther = question.value("allow_other").toBool(); form.required = question.value("required").toBool(true);
        const auto answer = draft.answers.value(form.id);
        auto *scroll = new QScrollArea; scroll->setFrameShape(QFrame::NoFrame); scroll->setWidgetResizable(true);
        scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
        auto *page = new QWidget; page->setObjectName("questionPage");
        auto *column = new QVBoxLayout(page); column->setContentsMargins(0, 0, 7, 0); column->setSpacing(7);
        auto *prompt = new QLabel(question.value("question").toString()); prompt->setObjectName("questionPrompt");
        prompt->setTextFormat(Qt::PlainText); prompt->setWordWrap(true); prompt->setTextInteractionFlags(Qt::TextSelectableByMouse);
        prompt->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
        column->addWidget(prompt);
        const QString body = question.value("body").toString();
        if (!body.isEmpty() && body != prompt->text()) {
            auto *detail = new QLabel(body); detail->setObjectName("questionBody"); detail->setTextFormat(Qt::PlainText); detail->setWordWrap(true);
            detail->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
            detail->setTextInteractionFlags(Qt::TextSelectableByMouse);
            if(m_question.value("approval").toBool()) {auto font=detail->font();font.setFamily("monospace");detail->setFont(font);}
            column->addWidget(detail);
        }
        auto *mode = new QLabel(form.multi ? tr("Select one or more") : tr("Select one")); mode->setObjectName("questionMode"); column->addWidget(mode);
        auto *group = new QButtonGroup(page); group->setExclusive(!form.multi);
        for (const auto &optionValue : question.value("options").toArray()) {
            const auto option = optionValue.toObject();
            auto *choice = new Choice(form.multi, option.value("label").toString(), option.value("description").toString(), page);
            choice->button->setObjectName("questionOption"); choice->button->setProperty("optionId", option.value("id").toString());
            choice->button->setProperty("questionId", form.id); group->addButton(choice->button);
            choice->button->setChecked(answer.options.contains(option.value("id").toString()));
            form.options.insert(option.value("id").toString(), choice->button); column->addWidget(choice);
            choice->setVisible(!m_question.value("approval").toBool());
            connect(choice->button, &QAbstractButton::toggled, this, [this] { capture(); });
        }
        if (form.allowOther) {
            if (!form.options.isEmpty()) {
                auto *other = new Choice(form.multi, tr("Other"), tr("Write your own answer"), page);
                form.other = other->button; form.other->setObjectName("questionOther"); form.other->setProperty("questionId", form.id);
                group->addButton(form.other); form.other->setChecked(answer.other); column->addWidget(other);
                connect(form.other, &QAbstractButton::toggled, this, [this] { capture(); });
            }
            form.text = new QLineEdit; form.text->setObjectName("questionFreeText"); form.text->setProperty("questionId", form.id);
            form.text->setPlaceholderText(tr("Write your answer…")); form.text->setAccessibleName(tr("Your answer to %1").arg(prompt->text()));
            form.text->setMinimumHeight(35); form.text->setMaxLength(4096); form.text->setText(answer.text);
            column->addWidget(form.text); connect(form.text, &QLineEdit::textChanged, this, [this] { capture(); });
        }
        mode->setVisible(!form.options.isEmpty()&&!m_question.value("approval").toBool()); column->addStretch(); scroll->setWidget(page);
        m_pages->addWidget(scroll); m_forms.append(form);
        const auto header = question.value("header").toString().simplified();
        const auto title = header.isEmpty() ? tr("Question %1").arg(++index) : QString::number(++index) + QStringLiteral(": ") + header.left(32);
        const int tab = m_tabs->addTab(title); m_tabs->setTabData(tab, title);
    }
    m_tabs->setVisible(m_tabs->count() > 1);
    if (m_tabs->count()) m_tabs->setCurrentIndex(qBound(0, draft.page, m_tabs->count() - 1));
    m_loading = false;
}

void QuestionCard::capture()
{
    if (m_loading || m_key.isEmpty()) return;
    auto &draft = m_drafts[m_key];
    for (const auto &form : m_forms) {
        auto &answer = draft.answers[form.id]; answer.options.clear();
        for (auto it = form.options.cbegin(); it != form.options.cend(); ++it) if (it.value()->isChecked()) answer.options.insert(it.key());
        answer.other = form.other && form.other->isChecked();
        if (form.text) answer.text = form.text->text();
    }
    updateControls();
}

QJsonArray QuestionCard::answers(bool *valid) const
{
    bool complete = !m_forms.isEmpty(); QJsonArray result;
    const auto draft = m_drafts.value(m_key);
    for (const auto &form : m_forms) {
        const auto answer = draft.answers.value(form.id); QJsonArray selected;
        // Return source option order, rather than hash/set iteration order.
        for (const auto &value : m_question.value("questions").toArray()) {
            const auto question = value.toObject(); if (question.value("id").toString() != form.id) continue;
            for (const auto &option : question.value("options").toArray()) {
                const auto id = option.toObject().value("id").toString(); if (answer.options.contains(id)) selected.append(id);
            }
        }
        const bool custom = form.allowOther && (form.options.isEmpty() || answer.other);
        const QString text = custom ? answer.text.trimmed() : QString();
        if ((selected.isEmpty() && text.isEmpty()) || (custom && text.isEmpty())
            || (!form.multi && selected.size() > 1) || !customAnswerProblem(text).isEmpty() || form.id.isEmpty()) complete = false;
        result.append(QJsonObject{{"question_id", form.id}, {"selected_option_ids", selected}, {"text", text}});
    }
    if (valid) *valid = complete;
    return result;
}

void QuestionCard::updateControls()
{
    const auto draft = m_drafts.value(m_key);
    const bool inFlight = m_sendingKeys.contains(identity(m_session, m_id));
    const bool locked = draft.sending || inFlight || draft.answered;
    // Disabling the focused answer control would move Qt focus out to the
    // context button. Hold it here until the card resolves or the user moves it.
    auto *focus = QApplication::focusWidget();
    if (locked && focus && (focus == m_submit || focus==m_approve || focus==m_deny || m_pages->isAncestorOf(focus)))
        setFocus(Qt::OtherFocusReason);
    bool complete = false; answers(&complete);
    for (const auto &form : m_forms) {
        for (auto *button : form.options) button->setEnabled(!locked);
        if (form.other) form.other->setEnabled(!locked);
        if (form.text) {
            const bool custom = form.options.isEmpty() || (form.other && form.other->isChecked());
            form.text->setVisible(custom); form.text->setEnabled(custom && !locked);
        }
    }
    const bool supported = m_question.value("can_answer").toBool();
    m_submit->setEnabled(!m_key.isEmpty() && m_available && supported && complete && !locked && !draft.uncertain);
    m_submit->setText(draft.sending || inFlight ? tr("Submitting…") : m_forms.size() == 1 ? tr("Submit answer") : tr("Submit answers"));
    m_retry->setVisible(draft.uncertain && !locked);
    const int page=m_tabs->currentIndex();
    m_previous->setVisible(m_forms.size()>1);m_previous->setEnabled(page>0&&!locked);
    m_next->setVisible(page>=0&&page<m_forms.size()-1);
    m_next->setEnabled(!locked);
    const bool approval=m_question.value("approval").toBool();
    const bool optional=m_question.value("optional").toBool();
    m_heading->setText(approval?tr("Agent needs your approval"):optional?tr("Optional question"):tr("Agent needs your answer"));
    if(optional && !approval && m_queueCount>1 && m_queueIndex>=0 && m_queueIndex<m_queueCount)
        m_heading->setText(tr("Optional question #%1/%2").arg(m_queueIndex+1).arg(m_queueCount));
    m_skip->setVisible(optional);
    m_skip->setEnabled(optional && m_available && m_question.value("can_skip").toBool() && !locked && !draft.uncertain);
    m_submit->setVisible(!approval&&page==m_forms.size()-1);
    m_approve->setVisible(approval);m_deny->setVisible(approval);
    const bool canApprove=approval&&m_available&&supported&&!locked&&!draft.uncertain;
    m_approve->setEnabled(canApprove&&m_reviewSeconds==0);m_deny->setEnabled(canApprove);
    m_approve->setText(m_reviewSeconds>0?tr("Approve once (%1)").arg(m_reviewSeconds):tr("Approve once"));
    int answered = 0;
    for (int i = 0; i < m_forms.size(); ++i) {
        const auto &form = m_forms[i]; const auto answer = draft.answers.value(form.id);
        const bool custom = form.allowOther && (form.options.isEmpty() || answer.other);
        const QString text = custom ? answer.text.trimmed() : QString();
        const bool filled = (!answer.options.isEmpty() || !text.isEmpty())
            && (!custom || !text.isEmpty()) && customAnswerProblem(text).isEmpty();
        if (filled) ++answered;
        m_tabs->setTabText(i, (filled ? QStringLiteral("✓ ") : QString()) + m_tabs->tabData(i).toString());
    }
    m_progress->setText(m_forms.isEmpty() ? QString() : tr("%1 of %2 answered").arg(answered).arg(m_forms.size()));
    m_progress->setVisible(!approval);
    QString notice = draft.notice;
    if (notice.isEmpty() && !m_available) notice = m_unavailableReason;
    if (notice.isEmpty() && !supported) notice = m_question.value("answer_unavailable_reason").toString(tr("Answer this request in Terminal."));
    if (notice.isEmpty()) for (const auto &form : m_forms) {
        const auto answer = draft.answers.value(form.id);
        if (!form.allowOther || (!form.options.isEmpty() && !answer.other)) continue;
        notice = customAnswerProblem(answer.text.trimmed());
        if (!notice.isEmpty()) break;
    }
    if (notice.isEmpty()) notice = approval?tr("Approval applies only to this command. Review it before continuing."):
        optional ? tr("The agent can continue. Answer when ready or skip this question.") :
        complete ? tr("Your choices are ready to send.") : tr("Answer each question to continue.");
    m_status->setText(notice.left(600));
    m_status->setStyleSheet(QString("color:%1;font-size:11px;").arg(draft.error ? (m_dark ? "#f4ab9b" : "#a13224") : (m_dark ? "#a2adbc" : "#627082")));
}

void QuestionCard::submit()
{
    if (!m_submit->isEnabled()) return;
    bool valid = false; const auto result = answers(&valid);
    if (valid) emit answerRequested(m_session, m_id, result);
}

void QuestionCard::paintEvent(QPaintEvent *)
{
    QStyleOption option; option.initFrom(this); QPainter painter(this);
    style()->drawPrimitive(QStyle::PE_Widget, &option, &painter, this);
}

void QuestionCard::setNativeUi(bool native) { m_terminal->setText(native ? tr("Open native UI") : tr("Open Terminal")); }

void QuestionCard::setTheme(bool dark)
{
    m_dark = dark;
    setStyleSheet(QString(R"(
        QWidget#questionCard { background:%1; border:1px solid %3; border-radius:10px; }
        QWidget#questionPage, QScrollArea { background:transparent; border:0; }
        QLabel { color:%2; border:0; }
        QLabel#questionHeading { color:%5; font-size:13px; font-weight:600; }
        QLabel#questionAskedAt, QLabel#questionPendingCount, QLabel#questionProgress, QLabel#questionMode, QLabel#questionBody, QLabel#questionChoiceDescription { color:%7; font-size:11px; }
        QLabel#questionPrompt { font-size:13px; font-weight:600; padding-bottom:3px; }
        QLabel#questionChoiceTitle { font-size:12px; }
        QFrame#questionChoice { background:%8; border:1px solid %3; border-radius:7px; }
        QFrame#questionChoice[checked=true] { background:%4; border-color:%5; }
        QFrame#questionChoice:hover { border-color:%5; }
        QCheckBox, QRadioButton { border:0; background:transparent; }
        QCheckBox::indicator, QRadioButton::indicator { width:12px; height:12px; border:1px solid %7; background:%8; }
        QRadioButton::indicator { border-radius:7px; }
        QCheckBox::indicator { border-radius:3px; }
        QCheckBox::indicator:checked, QRadioButton::indicator:checked { background:%5; border:2px solid %8; }
        QLineEdit { background:%8; color:%2; border:1px solid %3; border-radius:7px; padding:7px; font-size:12px; }
        QLineEdit:focus { border-color:%5; }
        QTabBar { font-size:11px; }
        QTabBar::tab { background:transparent; color:%7; padding:7px 10px; border-bottom:2px solid transparent; }
        QTabBar::tab:selected { color:%5; border-bottom-color:%5; }
        QPushButton#submitQuestionAnswer, QPushButton#approveToolRequest { background:%5; color:%6; border:1px solid %5; padding:7px 12px; border-radius:7px; font-weight:600; }
        QPushButton#submitQuestionAnswer:disabled, QPushButton#approveToolRequest:disabled { background:%8; color:%7; border-color:%3; }
        QPushButton#questionOpenTerminal, QPushButton#questionAllowRetry, QPushButton#questionPrevious, QPushButton#questionNext, QPushButton#denyToolRequest { padding:6px 9px; font-size:11px; color:%2; background:%8; border:1px solid %3; border-radius:6px; }
        QPushButton#questionNext { border-color:%5; color:%5; }
        QPushButton#questionPrevious:disabled, QPushButton#questionNext:disabled { color:%7; border-color:%3; }
        QToolButton#questionQueuePrevious, QToolButton#questionQueueNext { color:%2; background:%8; border:1px solid %3; border-radius:5px; padding:0; }
        QToolButton#questionQueuePrevious:hover, QToolButton#questionQueueNext:hover { border-color:%5; }
        QToolButton#questionQueuePrevious:disabled, QToolButton#questionQueueNext:disabled { color:%7; background:transparent; }
    )").arg(dark ? "#202a2c" : "#f4f9f7", dark ? "#e8edf4" : "#1a2733", dark ? "#40544e" : "#cbded4",
             dark ? "#293d35" : "#e4f3ec", dark ? "#8bdfc0" : "#167357", dark ? "#10231b" : "#ffffff",
             dark ? "#a2adbc" : "#627082", dark ? "#192228" : "#ffffff"));
    style()->unpolish(this); style()->polish(this);
    for (auto *child : findChildren<QWidget *>()) { child->style()->unpolish(child); child->style()->polish(child); }
    update();
    updateControls();
}
