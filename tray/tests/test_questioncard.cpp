#include "QuestionCard.h"

#include <QAbstractButton>
#include <QDir>
#include <QDateTime>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QSignalSpy>
#include <QTabBar>
#include <QTest>

namespace {
QJsonObject request(const QString &id = "tool-question", const QString &hash = "hash-one")
{
    return {{"question_id", id}, {"question_hash", hash}, {"run_id", "run"}, {"conversation_id", "conversation"},
        {"can_answer", true}, {"questions", QJsonArray{
            QJsonObject{{"id", "q_0"}, {"header", "Approach"}, {"question", "Which approach should I use?"}, {"allow_other", true},
                {"options", QJsonArray{
                    QJsonObject{{"id", "opt_0_0"}, {"label", "Keep it focused"}, {"description", "Implement the requested change with minimal dependencies."}},
                    QJsonObject{{"id", "opt_0_1"}, {"label", "Explore alternatives"}, {"description", "Compare multiple approaches before implementation."}}}}},
            QJsonObject{{"id", "q_1"}, {"header", "Validation"}, {"question", "Which checks should run?"}, {"multi_select", true}, {"allow_other", true},
                {"options", QJsonArray{
                    QJsonObject{{"id", "opt_1_0"}, {"label", "Unit tests"}}, QJsonObject{{"id", "opt_1_1"}, {"label", "Native UI preview"}}}}}}}};
}

QAbstractButton *option(QuestionCard &card, const QString &id)
{
    for (auto *button : card.findChildren<QAbstractButton *>("questionOption"))
        if (button->property("optionId").toString() == id) return button;
    return nullptr;
}
QAbstractButton *other(QuestionCard &card, const QString &id)
{
    for (auto *button : card.findChildren<QAbstractButton *>("questionOther"))
        if (button->property("questionId").toString() == id) return button;
    return nullptr;
}
QLineEdit *text(QuestionCard &card, const QString &id)
{
    for (auto *editor : card.findChildren<QLineEdit *>("questionFreeText"))
        if (editor->property("questionId").toString() == id) return editor;
    return nullptr;
}
QPushButton *submit(QuestionCard &card) { return card.findChild<QPushButton *>("submitQuestionAnswer"); }
void complete(QuestionCard &card) { option(card, "opt_0_0")->click(); option(card, "opt_1_0")->click(); }
}

class TestQuestionCard : public QObject {
    Q_OBJECT
private slots:
    void allRequiredQuestionsAndStableAnswerIds();
    void customAndMultipleAnswers();
    void draftsSurviveRefreshAndSessionSwitch();
    void replacementHashAndBackgroundReplyAreIsolated();
    void capabilityOfflineAndErrorStates();
    void freeformLimitsAndPlainText();
    void longProviderTextKeepsCompactWidth();
    void reviewCountdownAndNextNavigation();
    void optionalQuestionsHaveExplicitSkipAndKeepDrafts();
    void hookTrustRequiresAnExplicitChoice();
    void preview();
};

void TestQuestionCard::hookTrustRequiresAnExplicitChoice()
{
    QuestionCard card;auto hooks=request();hooks["trust_request"]=true;hooks["conversation_id"]=QJsonValue::Null;
    hooks["questions"]=QJsonArray{QJsonObject{{"id","hooks_trust"},{"header","Hook trust"},{"question","Hooks need review"},
        {"body","4 hooks are new or changed. Hooks can run outside the sandbox after you trust them."},{"allow_other",false},
        {"options",QJsonArray{QJsonObject{{"id","review"},{"label","Review hooks"}},QJsonObject{{"id","trust"},{"label","Trust all and continue"}},
            QJsonObject{{"id","continue_without_trusting"},{"label","Continue without trusting (hooks won't run)"}}}}}};
    card.setQuestion("startup",hooks);card.resize(700,450);card.show();
    QCOMPARE(card.findChild<QLabel *>("questionHeading")->text(),QString("Agent needs your approval"));
    QVERIFY(card.findChild<QPushButton *>("skipQuestion")->isHidden());
    QVERIFY(!submit(card)->isEnabled());QSignalSpy sent(&card,&QuestionCard::answerRequested);
    option(card,"trust")->click();QVERIFY(submit(card)->isEnabled());QCOMPARE(sent.size(),0);
    submit(card)->click();QCOMPARE(sent.size(),1);
    QCOMPARE(sent.first()[2].toJsonArray().first().toObject()["selected_option_ids"].toArray(),QJsonArray{"trust"});
    const auto preview=qEnvironmentVariable("ZERUS_HOOKS_PREVIEW");
    if(!preview.isEmpty()) {QDir().mkpath(preview);QVERIFY(card.grab().save(preview+"/hooks.png"));}
}

void TestQuestionCard::reviewCountdownAndNextNavigation()
{
    QuestionCard card;card.resize(700,450);card.setQuestion("session",request());card.show();card.activateWindow();QApplication::setActiveWindow(&card);
    auto *next=card.findChild<QPushButton *>("questionNext"),*back=card.findChild<QPushButton *>("questionPrevious");
    QVERIFY(next->isVisible());QVERIFY(!back->isEnabled());QVERIFY(submit(card)->isHidden());
    option(card,"opt_0_0")->click();next->click();
    QCOMPARE(card.findChild<QTabBar *>("questionTabs")->currentIndex(),1);QVERIFY(next->isHidden());QVERIFY(submit(card)->isVisible());
    option(card,"opt_1_0")->click();back->click();QVERIFY(option(card,"opt_0_0")->isChecked());
    auto approval=request("approval","command-one");approval["approval"]=true;
    approval["questions"]=QJsonArray{QJsonObject{{"id","approval"},{"question","Approve this Bash command?"},{"body","printf fixture > fixture.txt"},
        {"options",QJsonArray{QJsonObject{{"id","allow"},{"label","Approve once"}},QJsonObject{{"id","deny"},{"label","Deny"}}}}}};
    card.setQuestion("session",approval);
    auto *allow=card.findChild<QPushButton *>("approveToolRequest");auto *deny=card.findChild<QPushButton *>("denyToolRequest");
    QSignalSpy sent(&card,&QuestionCard::answerRequested);
    QVERIFY(!allow->isEnabled());QVERIFY(deny->isEnabled());QVERIFY(allow->text().contains("(3)"));
    QTest::qWait(1100);card.setQuestion("session",approval);QVERIFY(!allow->isEnabled());
    card.hide();QTest::qWait(150);card.show();card.activateWindow();QApplication::setActiveWindow(&card);QVERIFY(!allow->isEnabled());
    QTRY_VERIFY_WITH_TIMEOUT(allow->isEnabled(),4000);QCOMPARE(sent.size(),0);
    approval["question_hash"]="command-two";card.setQuestion("session",approval);QVERIFY(!allow->isEnabled());
    QTRY_VERIFY_WITH_TIMEOUT(allow->isEnabled(),4000);allow->click();QCOMPARE(sent.size(),1);
    QCOMPARE(sent.first()[2].toJsonArray().first().toObject().value("selected_option_ids").toArray(),QJsonArray{"allow"});
    const auto preview=qEnvironmentVariable("HGS_APPROVAL_PREVIEW");if(!preview.isEmpty()){QDir().mkpath(preview);QVERIFY(card.grab().save(preview+"/approval.png"));}
}

void TestQuestionCard::allRequiredQuestionsAndStableAnswerIds()
{
    QuestionCard card; card.setQuestion("arch/session", request());
    QSignalSpy submitted(&card, &QuestionCard::answerRequested);
    QVERIFY(!submit(card)->isEnabled());
    option(card, "opt_0_1")->click(); QVERIFY(!submit(card)->isEnabled());
    QVERIFY(card.findChild<QTabBar *>("questionTabs")->tabText(0).startsWith(QStringLiteral("✓")));
    QCOMPARE(card.findChild<QLabel *>("questionProgress")->text(), "1 of 2 answered");
    card.findChild<QTabBar *>("questionTabs")->setCurrentIndex(1);
    option(card, "opt_1_1")->click(); QVERIFY(submit(card)->isEnabled());
    submit(card)->click(); QCOMPARE(submitted.size(), 1);
    QCOMPARE(submitted[0][0].toString(), "arch/session"); QCOMPARE(submitted[0][1].toString(), "tool-question");
    const auto answers = submitted[0][2].toJsonArray(); QCOMPARE(answers.size(), 2);
    QCOMPARE(answers[0].toObject()["question_id"].toString(), "q_0");
    QCOMPARE(answers[0].toObject()["selected_option_ids"].toArray(), QJsonArray{"opt_0_1"});
    QCOMPARE(answers[1].toObject()["selected_option_ids"].toArray(), QJsonArray{"opt_1_1"});
    QVERIFY(answers[0].toObject()["text"].toString().isEmpty());
}

void TestQuestionCard::customAndMultipleAnswers()
{
    QuestionCard card; card.setQuestion("arch/session", request());
    QSignalSpy submitted(&card, &QuestionCard::answerRequested);
    complete(card); other(card, "q_0")->click();
    QVERIFY(!option(card, "opt_0_0")->isChecked()); QVERIFY(!submit(card)->isEnabled());
    text(card, "q_0")->setText("  My approach  "); QVERIFY(submit(card)->isEnabled());
    option(card, "opt_1_1")->click(); other(card, "q_1")->click(); QVERIFY(!submit(card)->isEnabled());
    text(card, "q_1")->setText("Also check keyboard navigation");
    submit(card)->click(); QCOMPARE(submitted.size(), 1);
    const auto answers = submitted[0][2].toJsonArray();
    QVERIFY(answers[0].toObject()["selected_option_ids"].toArray().isEmpty());
    QCOMPARE(answers[0].toObject()["text"].toString(), "My approach");
    QCOMPARE(answers[1].toObject()["selected_option_ids"].toArray(), QJsonArray({"opt_1_0", "opt_1_1"}));
    QCOMPARE(answers[1].toObject()["text"].toString(), "Also check keyboard navigation");
}

void TestQuestionCard::draftsSurviveRefreshAndSessionSwitch()
{
    QuestionCard card; card.setQuestion("arch/session", request()); complete(card);
    other(card, "q_0")->click(); text(card, "q_0")->setText("Remember this");
    card.findChild<QTabBar *>("questionTabs")->setCurrentIndex(1);
    auto *editor = text(card, "q_0"); editor->setSelection(0, 8);
    card.setQuestion("arch/session", request());
    QCOMPARE(text(card, "q_0"), editor); QCOMPARE(editor->selectedText(), "Remember");
    card.setQuestion("mac/session", request()); QVERIFY(!submit(card)->isEnabled());
    QVERIFY(text(card, "q_0")->text().isEmpty());
    card.setQuestion("arch/session", request());
    QCOMPARE(text(card, "q_0")->text(), "Remember this"); QVERIFY(submit(card)->isEnabled());
    QCOMPARE(card.findChild<QTabBar *>("questionTabs")->currentIndex(), 1);
    card.setQuestion({}, {}); QVERIFY(card.isHidden());
    card.setQuestion("arch/session", request()); QVERIFY(submit(card)->isEnabled());
}

void TestQuestionCard::replacementHashAndBackgroundReplyAreIsolated()
{
    QuestionCard card; card.setQuestion("arch/session", request()); complete(card);
    card.setSending("arch/session", "tool-question");
    QVERIFY(!submit(card)->isEnabled()); QVERIFY(!option(card, "opt_0_0")->isEnabled());
    card.setQuestion("mac/session", request()); complete(card);
    card.setAnswered("arch/session", "tool-question"); QVERIFY(submit(card)->isEnabled());
    card.setQuestion("arch/session", request()); QVERIFY(!submit(card)->isEnabled());
    QVERIFY(card.findChild<QLabel *>("questionStatus")->text().contains("Answer sent"));
    card.setQuestion("arch/session", request("tool-question", "hash-two"));
    QVERIFY(!option(card, "opt_0_0")->isChecked()); complete(card);
    card.setSending("arch/session", "tool-question");
    card.setQuestion("arch/session", request("tool-question", "hash-three"));
    // Keep a replacement request locked until the old hash's callback arrives.
    QVERIFY(!option(card, "opt_0_0")->isEnabled());
    card.setAnswered("arch/session", "tool-question");
    QVERIFY(option(card, "opt_0_0")->isEnabled()); complete(card); QVERIFY(submit(card)->isEnabled());
}

void TestQuestionCard::capabilityOfflineAndErrorStates()
{
    QuestionCard card; auto unsupported = request(); unsupported["can_answer"] = false;
    unsupported["answer_unavailable_reason"] = "This agent requires Terminal.";
    card.setQuestion("arch/session", unsupported); complete(card);
    QVERIFY(!submit(card)->isEnabled()); QCOMPARE(card.findChild<QLabel *>("questionStatus")->text(), "This agent requires Terminal.");
    QSignalSpy terminal(&card, &QuestionCard::openTerminalRequested);
    card.findChild<QPushButton *>("questionOpenTerminal")->click(); QCOMPARE(terminal.size(), 1);
    card.setQuestion("arch/session", request()); QVERIFY(submit(card)->isEnabled());
    card.setAvailability(false, "Machine is offline."); QVERIFY(!submit(card)->isEnabled());
    QCOMPARE(card.findChild<QLabel *>("questionStatus")->text(), "Machine is offline.");
    card.setAvailability(true);
    card.setSending("arch/session", "tool-question");
    card.setError("arch/session", "tool-question", "Cannot confirm delivery", true);
    QVERIFY(!submit(card)->isEnabled()); QVERIFY(option(card, "opt_0_0")->isChecked());
    QVERIFY(card.findChild<QPushButton *>("questionAllowRetry")->isVisible());
    card.findChild<QPushButton *>("questionAllowRetry")->click(); QVERIFY(submit(card)->isEnabled());
    card.setSending("arch/session", "tool-question"); card.setError("arch/session", "tool-question", "Request changed");
    QVERIFY(submit(card)->isEnabled()); QCOMPARE(card.findChild<QLabel *>("questionStatus")->text(), "Request changed");
}

void TestQuestionCard::freeformLimitsAndPlainText()
{
    QuestionCard card; auto data = request();
    data["questions"] = QJsonArray{QJsonObject{{"id", "free"}, {"question", "<img src='file:///private'>"}, {"allow_other", true}, {"options", QJsonArray{}}}};
    card.setQuestion("arch/session", data);
    const auto *prompt = card.findChild<QLabel *>("questionPrompt");
    QCOMPARE(prompt->textFormat(), Qt::PlainText); QVERIFY(prompt->text().contains("<img"));
    QVERIFY(!submit(card)->isEnabled());
    text(card, "free")->setText(QString(2048, QChar(0x044f))); QVERIFY(submit(card)->isEnabled());
    text(card, "free")->setText(QString(2049, QChar(0x044f))); QVERIFY(!submit(card)->isEnabled());
    text(card, "free")->setText("one\ntwo"); QVERIFY(!submit(card)->isEnabled());
    QVERIFY(card.findChild<QLabel *>("questionStatus")->text().contains("one line"));
    text(card, "free")->setText(" /exit"); QVERIFY(!submit(card)->isEnabled());
    QVERIFY(card.findChild<QLabel *>("questionStatus")->text().contains("Terminal"));
    text(card, "free")->setText(QStringLiteral("before") + QChar(1) + "after"); QVERIFY(!submit(card)->isEnabled());
    QVERIFY(card.findChild<QLabel *>("questionStatus")->text().contains("control characters"));
    text(card, "free")->setText("single line"); QVERIFY(submit(card)->isEnabled());
}

void TestQuestionCard::longProviderTextKeepsCompactWidth()
{
    auto data = request();
    const QString token(8000, QChar('x'));
    data["questions"] = QJsonArray{QJsonObject{{"id", "long"}, {"question", token}, {"body", "body" + token},
        {"options", QJsonArray{QJsonObject{{"id", "long-option"}, {"label", token}, {"description", token}}}}}};
    QuestionCard card; card.resize(480, 360); card.setQuestion("arch/session", data); card.show();
    QTest::qWait(25); QCOMPARE(card.width(), 480);
    const auto *scroll = card.findChild<QScrollArea *>(); QVERIFY(scroll);
    QVERIFY(scroll->widget()->width() <= scroll->viewport()->width());
    for (const auto *label : card.findChildren<QLabel *>())
        if (label->isVisible()) QVERIFY(label->width() < card.width());
    card.setError("arch/session", "tool-question", token); QTest::qWait(25);
    QCOMPARE(card.width(), 480);
    QVERIFY(card.findChild<QLabel *>("questionStatus")->width() < card.width());
}

void TestQuestionCard::preview()
{
    const auto directory = qEnvironmentVariable("HGS_PREVIEW_DIR");
    if (directory.isEmpty()) QSKIP("Set HGS_PREVIEW_DIR to render question form previews");
    QDir().mkpath(directory);
    for (const bool dark : {true, false}) {
        QuestionCard card; card.resize(620, 420); card.setTheme(dark); card.setQuestion("arch/session", request());
        complete(card); card.show(); QTest::qWait(25);
        QVERIFY(card.styleSheet().contains(dark ? "#202a2c" : "#f4f9f7"));
        QCOMPARE(card.grab().toImage().pixelColor(600, 345), QColor(dark ? "#202a2c" : "#f4f9f7"));
        QVERIFY(card.grab().save(directory + (dark ? "/question-dark.png" : "/question-light.png")));
        card.resize(480, 360); card.findChild<QTabBar *>("questionTabs")->setCurrentIndex(1); QTest::qWait(20);
        QVERIFY(card.grab().save(directory + (dark ? "/question-dark-compact.png" : "/question-light-compact.png")));
        auto optional=request("queued");optional["optional"]=true;optional["can_skip"]=true;
        optional["created_at"]=double(QDateTime(QDate(2026,10,7),QTime(12,34,56)).toSecsSinceEpoch());
        optional["questions"]=QJsonArray{optional["questions"].toArray().first()};
        card.setQuestion("arch/session",optional,3);card.setQueueNavigation(1,3);QTest::qWait(20);
        QVERIFY(card.grab().save(directory + (dark ? "/question-dark-queue.png" : "/question-light-queue.png")));
    }
}

QTEST_MAIN(TestQuestionCard)
#include "test_questioncard.moc"

void TestQuestionCard::optionalQuestionsHaveExplicitSkipAndKeepDrafts()
{
    QuestionCard card;auto data=request();data["optional"]=true;data["can_skip"]=true;
    auto item=data["questions"].toArray().first().toObject();item["required"]=false;
    const auto created=QDateTime(QDate(2026,10,7),QTime(12,34,56)).toSecsSinceEpoch();data["created_at"]=double(created);
    data["questions"]=QJsonArray{item};card.setQuestion("session",data,3);card.show();
    auto *asked=card.findChild<QLabel *>("questionAskedAt"),*pending=card.findChild<QLabel *>("questionPendingCount");
    QVERIFY(asked->isVisible());QVERIFY(asked->text().contains("7 Oct"));QVERIFY(asked->text().contains("12:34:56"));QCOMPARE(pending->text(),QString("3 pending"));
    const auto timestamp=asked->text();
    auto *skip=card.findChild<QPushButton *>("skipQuestion");QVERIFY(skip->isVisible());QVERIFY(skip->isEnabled());
    QVERIFY(!submit(card)->isEnabled());QSignalSpy sent(&card,&QuestionCard::answerRequested);
    other(card,"q_0")->click();text(card,"q_0")->setText("Draft answer");
    card.setQuestion("session",data,2);QCOMPARE(text(card,"q_0")->text(),QString("Draft answer"));
    QCOMPARE(asked->text(),timestamp);QCOMPARE(pending->text(),QString("2 pending"));
    data["created_at"]=0;card.setQuestion("session",data,2);QVERIFY(asked->isHidden());QVERIFY(asked->text().isEmpty());
    QCOMPARE(text(card,"q_0")->text(),QString("Draft answer"));
    data["can_answer"]=false;card.setQuestion("session",data);QVERIFY(skip->isEnabled());QVERIFY(!submit(card)->isEnabled());
    card.resize(620,420);QTest::qWait(1);
    const auto submitBounds=submit(card)->geometry(),skipBounds=skip->geometry();
    skip->click();QCOMPARE(sent.size(),1);QCOMPARE(sent.first()[2].toJsonArray(),QJsonArray{QJsonObject({{"question_id","q_0"},{"skip",true}})});
    card.setSending("session",data["question_id"].toString());QTest::qWait(1);
    QCOMPARE(submit(card)->text(),QString("Submitting…"));
    QCOMPARE(submit(card)->geometry(),submitBounds);QCOMPARE(skip->geometry(),skipBounds);
    card.setError("session",data["question_id"].toString(),"Try again");QTest::qWait(1);
    QCOMPARE(submit(card)->geometry(),submitBounds);QCOMPARE(skip->geometry(),skipBounds);
    card.setAvailability(false,"Offline");QVERIFY(!skip->isEnabled());
    card.setAvailability(true);card.setQuestion("session",request());QVERIFY(skip->isHidden());
}
