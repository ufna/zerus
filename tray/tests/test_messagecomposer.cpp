#include "MessageComposer.h"

#include <QApplication>
#include <QComboBox>
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDragLeaveEvent>
#include <QDropEvent>
#include <QFile>
#include <QFontInfo>
#include <QFrame>
#include <QImage>
#include <QInputMethodEvent>
#include <QLabel>
#include <QMimeData>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QTemporaryDir>
#include <QSignalSpy>
#include <QScopeGuard>
#include <QTest>
#include <QStyleOptionComboBox>
#include <QUrl>
#include <QVBoxLayout>
#include <QToolTip>

namespace {
struct Submission {
    QString key, text;
    QList<MessageAttachment> attachments;
};

void observe(MessageComposer &composer, QList<Submission> &submissions)
{
    QObject::connect(&composer, &MessageComposer::sendRequested, &composer,
        [&submissions](const QString &key, const QString &text, const QList<MessageAttachment> &attachments) {
            submissions.append({key, text, attachments});
        });
}

QPushButton *sendButton(MessageComposer &composer) { return composer.findChild<QPushButton *>("sendMessage"); }
QLabel *status(MessageComposer &composer) { return composer.findChild<QLabel *>("messageStatus"); }

// Exercise the editor's MIME insertion path without reading or overwriting the
// user's real clipboard. A drop forwards the same QMimeData into that path.
void drop(QPlainTextEdit *editor, const QMimeData &mime)
{
    const QPoint position = editor->cursorRect().center();
    QDragEnterEvent enter(position, Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(editor->viewport(), &enter);
    QVERIFY2(enter.isAccepted(), "Editor should accept supported synthetic MIME data");
    QDragMoveEvent move(position, Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(editor->viewport(), &move);
    QVERIFY2(move.isAccepted(), "Editor should show the native drop insertion position");
    QDropEvent drop(QPointF(position), Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(editor->viewport(), &drop);
    QVERIFY2(drop.isAccepted(), "Editor should accept the synthetic drop");
}
}

class TestMessageComposer : public QObject {
    Q_OBJECT
private slots:
    void isolatesDraftsAcrossSessions();
    void enterSendsShiftEnterAddsLine();
    void inputMethodDoesNotAccidentallySubmit();
    void availabilityAndBusyGateSubmission();
    void syntheticImageAndFileDrop();
    void inlineReferencesFollowCursorAndSurviveEditing();
    void escapeReleasesFocusWithoutChangingDraft();
    void attachmentLimits();
    void textLimitCountsUtf8Bytes();
    void failureAndUncertaintyPreserveDraft();
    void sendingHidesPayloadAndRestoresFailedBackgroundDelivery();
    void backgroundSuccessDoesNotClearVisibleDraft();
    void renamePreservesDraft();
    void retryOfAnotherMessagePreservesDraftAndUncertainty();
    void modelSettingsPreserveDraftAndDistinguishPending();
    void modelSettingsUnavailableExplainsWhy();
    void modelSettingsFitNarrowColumn();
    void cursorStaysWithDraftAcrossPollingSwitchesAndFailures();
    void attachmentDropReleasesVisualCaret_data();
    void attachmentDropReleasesVisualCaret();
    void unsupportedModelOffersTerminal();
    void contentScaleEnlargesOnlyTheMessageField();
    void draftStateFollowsUnsentContent();
};

void TestMessageComposer::contentScaleEnlargesOnlyTheMessageField()
{
    QWidget workspace; workspace.setStyleSheet("QWidget { font-size:13px; }");
    auto *layout = new QVBoxLayout(&workspace); auto *composer = new MessageComposer; layout->addWidget(composer);
    composer->setTheme(false); composer->setSessionKey("local/scale"); composer->setAvailability(true); composer->editor()->setPlainText("draft survives");
    workspace.resize(520, 300); workspace.show(); QTest::qWait(10);
    auto *send = composer->findChild<QPushButton *>("sendMessage"); QVERIFY(send);
    const int height = composer->editor()->height(), sendFont = QFontInfo(send->font()).pixelSize();
    QCOMPARE(QFontInfo(composer->editor()->font()).pixelSize(), 13);
    composer->setContentScale(2.0); QTest::qWait(10);
    QCOMPARE(QFontInfo(composer->editor()->font()).pixelSize(), 26);
    QCOMPARE(composer->editor()->height(), 2 * height);
    QCOMPARE(QFontInfo(send->font()).pixelSize(), sendFont);
    QCOMPARE(composer->editor()->toPlainText(), QString("draft survives"));
    composer->setTheme(true);
    QCOMPARE(QFontInfo(composer->editor()->font()).pixelSize(), 26);
    composer->setContentScale(1.0); QTest::qWait(10);
    QCOMPARE(QFontInfo(composer->editor()->font()).pixelSize(), 13);
    QCOMPARE(composer->editor()->height(), height);
}

void TestMessageComposer::attachmentDropReleasesVisualCaret_data()
{
    QTest::addColumn<bool>("image");
    QTest::newRow("image") << true;
    QTest::newRow("file") << false;
}

void TestMessageComposer::attachmentDropReleasesVisualCaret()
{
    QFETCH(bool, image);
    const int flashTime = QApplication::cursorFlashTime();
    const auto restoreFlash = qScopeGuard([flashTime] { QApplication::setCursorFlashTime(flashTime); });
    QApplication::setCursorFlashTime(0);
    MessageComposer composer; composer.resize(900, 240); composer.setSessionKey("one"); composer.setAvailability(true);
    composer.show(); QVERIFY(QTest::qWaitForWindowExposed(&composer)); composer.activateWindow();
    auto *editor = composer.editor(); editor->setFocus(); QTRY_VERIFY(editor->hasFocus());
    editor->insertPlainText(QString::fromUtf8("Проверь релиз и документ"));
    QTemporaryDir dir; QVERIFY(dir.isValid());
    QMimeData mime;
    if (image) {
        QImage pixels(4, 4, QImage::Format_RGB32); pixels.fill(Qt::red); mime.setImageData(pixels);
    } else {
        QFile file(dir.filePath("review.docx")); QVERIFY(file.open(QIODevice::WriteOnly)); file.write("document"); file.close();
        mime.setUrls({QUrl::fromLocalFile(file.fileName())});
    }
    drop(editor, mime);
    const QString draft = editor->toPlainText();
    const QRect end = editor->cursorRect();
    const auto endImage = editor->viewport()->grab().toImage();
    QTest::keyClick(editor, Qt::Key_Home);
    QCOMPARE(editor->textCursor().position(), 0);
    const auto homeImage = editor->viewport()->grab().toImage();
    // Checking textCursor() alone misses Qt's separate drag-feedback cursor:
    // it can override the painted caret even while insertion moves correctly.
    QVERIFY2(homeImage != endImage, "The painted caret must move with Home after an attachment drop");
    QDragLeaveEvent cleanup; QApplication::sendEvent(editor->viewport(), &cleanup);
    QCOMPARE(editor->viewport()->grab().toImage(), homeImage);
    QTest::keyClicks(editor, "X"); QCOMPARE(editor->toPlainText(), "X" + draft);
    QTest::mouseClick(editor->viewport(), Qt::LeftButton, Qt::NoModifier, end.center());
    const int insertion = editor->textCursor().position(); QVERIFY(insertion > 1);
    const auto clickImage = editor->viewport()->grab().toImage();
    QTest::keyClick(editor, Qt::Key_Home);
    QVERIFY(editor->viewport()->grab().toImage() != clickImage);
    QTest::mouseClick(editor->viewport(), Qt::LeftButton, Qt::NoModifier, end.center());
    auto expected = editor->toPlainText(); expected.insert(editor->textCursor().position(), "Y");
    QTest::keyClicks(editor, "Y"); QCOMPARE(editor->toPlainText(), expected);
}

void TestMessageComposer::cursorStaysWithDraftAcrossPollingSwitchesAndFailures()
{
    MessageComposer composer; composer.resize(700,200); composer.setSessionKey("one"); composer.setAvailability(true);composer.show();QVERIFY(QTest::qWaitForWindowExposed(&composer));composer.activateWindow();
    auto *editor=composer.editor();editor->setFocus();QTRY_VERIFY(editor->hasFocus());
    QVERIFY(composer.addAttachment("review.docx","application/octet-stream","document"));
    editor->insertPlainText(QString::fromUtf8("Изучи этот документ и подготовь ответ"));
    auto cursor=editor->textCursor();cursor.setPosition(16);cursor.setPosition(20,QTextCursor::KeepAnchor);editor->setTextCursor(cursor);
    const auto draft=editor->toPlainText();const auto rectangle=editor->cursorRect();
    for (int i=0;i<3;++i) {composer.setSessionKey("one");composer.setAvailability(true);composer.setModelSettings("claude-opus-5-5","high",{},false,"Use Terminal");QCoreApplication::processEvents();}
    QCOMPARE(editor->textCursor().position(),20);QCOMPARE(editor->textCursor().anchor(),16);QCOMPARE(editor->cursorRect(),rectangle);
    composer.setSessionKey("two");editor->setPlainText("Other draft");composer.setSessionKey("one");
    QCOMPARE(editor->toPlainText(),draft);QCOMPARE(editor->textCursor().position(),20);QCOMPARE(editor->textCursor().anchor(),16);
    composer.setSending("one");composer.deliveryFinished("one",false,"Terminal is busy");
    QCOMPARE(editor->toPlainText(),draft);QCOMPARE(editor->textCursor().position(),20);QCOMPARE(editor->textCursor().anchor(),16);
    QTest::keyClick(editor,Qt::Key_Right);QTest::keyClick(editor,Qt::Key_Home);
    QCOMPARE(editor->textCursor().position(),0);
    QTest::keyClicks(editor,"X");QCOMPARE(editor->toPlainText(),"X"+draft);
    const auto target=editor->cursorRect();QVERIFY(target.x()<rectangle.x());
    QTest::mouseClick(editor->viewport(),Qt::LeftButton,Qt::NoModifier,rectangle.center());
    const int insertion=editor->textCursor().position();const auto before=editor->toPlainText();
    QTest::keyClicks(editor,"Y");auto expected=before;expected.insert(insertion,"Y");QCOMPARE(editor->toPlainText(),expected);
    QVERIFY(composer.findChild<QPushButton *>("removeAttachment"));
}

void TestMessageComposer::unsupportedModelOffersTerminal()
{
    MessageComposer composer;composer.setSessionKey("claude/one");composer.setAvailability(true);composer.show();
    composer.editor()->setPlainText("Keep draft");composer.setSettingsTerminalAvailable(true);
    composer.setModelSettings("claude-opus-5-5","high",{},false,"Change model with /model and effort with /effort in Terminal.");
    QSignalSpy terminal(&composer,&MessageComposer::settingsTerminalRequested);
    composer.findChild<QPushButton *>("sessionModelSettings")->click();
    auto *button=composer.findChild<QPushButton *>("sessionSettingsTerminal");QVERIFY(button->isVisible());
    QVERIFY(!composer.findChild<QComboBox *>("sessionModelChoice")->isVisible());
    QVERIFY(!composer.findChild<QPushButton *>("applySessionSettings")->isVisible());
    button->click();QCOMPARE(terminal.size(),1);QCOMPARE(terminal[0][0].toString(),QString("claude/one"));QCOMPARE(composer.editor()->toPlainText(),QString("Keep draft"));
}

void TestMessageComposer::escapeReleasesFocusWithoutChangingDraft()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("one"); composer.setAvailability(true); composer.show(); composer.activateWindow();
    composer.editor()->setPlainText("Keep this message");
    QVERIFY(composer.addAttachment("screen.png", "image/png", "image"));
    const auto draft = composer.editor()->toPlainText();
    composer.editor()->setFocus(); QTRY_VERIFY(composer.editor()->hasFocus());
    QTest::keyClick(composer.editor(), Qt::Key_Escape);
    QVERIFY(!composer.editor()->hasFocus()); QVERIFY(composer.isVisible());
    QCOMPARE(composer.editor()->toPlainText(), draft);
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 1);
    QVERIFY(submissions.isEmpty());
}

void TestMessageComposer::inlineReferencesFollowCursorAndSurviveEditing()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("one"); composer.setAvailability(true);
    composer.editor()->setPlainText(QString::fromUtf8("До\nпосле"));
    auto cursor = composer.editor()->textCursor(); cursor.setPosition(3); composer.editor()->setTextCursor(cursor);
    QVERIFY(composer.addAttachment("same.png", "image/png", "first"));
    QCOMPARE(composer.editor()->toPlainText(), QString::fromUtf8("До\n[Image #1] после"));
    cursor = composer.editor()->textCursor(); cursor.movePosition(QTextCursor::End); composer.editor()->setTextCursor(cursor);
    QVERIFY(composer.addAttachment("same.png", "image/png", "second"));
    QCOMPARE(composer.editor()->toPlainText(), QString::fromUtf8("До\n[Image #1] после [Image #2] "));
    const auto preview = qEnvironmentVariable("HGS_COMPOSER_PREVIEW");
    if (!preview.isEmpty()) {
        composer.resize(760, 200); composer.show(); QTest::qWait(30);
        QVERIFY(composer.grab().save(preview));
    }
    // Moving labels changes their position without renumbering the attachments.
    composer.editor()->setPlainText("Second [Image #2]\nfirst [Image #1]");
    composer.setSessionKey("two"); composer.setSessionKey("one");
    sendButton(composer)->click(); QCOMPARE(submissions.size(), 1);
    QCOMPARE(submissions[0].text, QString("Second [Image #2]\nfirst [Image #1]"));
    QCOMPARE(submissions[0].attachments[0].reference, QString("[Image #1]"));
    QCOMPARE(submissions[0].attachments[1].reference, QString("[Image #2]"));
    composer.setSending("one"); composer.deliveryFinished("one", false, "Try again");
    QCOMPARE(composer.editor()->toPlainText(), submissions[0].text);
    composer.findChildren<QPushButton *>("removeAttachment").first()->click();
    QVERIFY(!composer.editor()->toPlainText().contains("[Image #1]"));
    QVERIFY(composer.editor()->toPlainText().contains("[Image #2]"));
    QVERIFY(composer.addAttachment("third.png", "image/png", "third"));
    QVERIFY(composer.editor()->toPlainText().contains("[Image #3]"));
    QVERIFY(!composer.editor()->toPlainText().contains("[Image #1]"));
    sendButton(composer)->click();
    QCOMPARE(submissions.last().attachments[0].reference, QString("[Image #2]"));
    QCOMPARE(submissions.last().attachments[1].reference, QString("[Image #3]"));
    composer.setSending("one"); composer.deliveryFinished("one", true);
    QVERIFY(composer.addAttachment("new.png", "image/png", "new"));
    QCOMPARE(composer.editor()->toPlainText(), QString("[Image #1] "));
}

void TestMessageComposer::retryOfAnotherMessagePreservesDraftAndUncertainty()
{
    MessageComposer composer;composer.setSessionKey("one");composer.setAvailability(true);
    composer.editor()->setPlainText("Unrelated draft");QVERIFY(composer.addAttachment("next.txt","text/plain","next"));
    const QList<MessageAttachment> files{{"next.txt","text/plain","next","[File #1]"}};
    QVERIFY(composer.draftMatches("one","[File #1] Unrelated draft",files));
    composer.deliveryFinished("one",false,"Check the previous delivery",true);
    composer.setSending("one",true);composer.setSessionKey("two");composer.editor()->setPlainText("Other session draft");
    composer.deliveryFinished("one",true);QCOMPARE(composer.editor()->toPlainText(),QString("Other session draft"));
    composer.setSessionKey("one");QVERIFY(composer.draftMatches("one","[File #1] Unrelated draft",files));
    QCOMPARE(composer.editor()->toPlainText(),QString("[File #1] Unrelated draft"));QVERIFY(!sendButton(composer)->isEnabled());
    QVERIFY(status(composer)->text().contains("Check the previous delivery"));
}

void TestMessageComposer::modelSettingsPreserveDraftAndDistinguishPending()
{
    MessageComposer composer; composer.resize(640, 220); composer.setSessionKey("arch/session");
    const QJsonArray options{QJsonObject{{"id", "model-one"}, {"effort_options", QJsonArray{"low", "high", "xhigh"}}},
        QJsonObject{{"id", "model-two"}, {"label", "Second model"}, {"effort_options", QJsonArray{"low", "high"}}}};
    composer.setModelSettings("model-one", "xhigh", options, true, {}, {}, {}, "resume");
    composer.editor()->setPlainText("Keep this draft"); QVERIFY(composer.addAttachment("keep.txt", "text/plain", "keep"));
    QSignalSpy changes(&composer, &MessageComposer::settingsRequested);
    composer.show(); QTest::qWait(20);
    auto *button = composer.findChild<QPushButton *>("sessionModelSettings"); QVERIFY(button); button->click();
    auto *models = composer.findChild<QComboBox *>("sessionModelChoice");
    auto *efforts = composer.findChild<QComboBox *>("sessionEffortChoice");
    auto *apply = composer.findChild<QPushButton *>("applySessionSettings");
    auto *cancel = composer.findChild<QPushButton *>("cancelSessionSettings"); QVERIFY(cancel);
    auto *popup = composer.findChild<QFrame *>("sessionSettingsPopup");
    QCOMPARE(efforts->currentData().toString(), QString("xhigh")); QVERIFY(!apply->isEnabled());
    models->setCurrentIndex(models->findData("model-two")); cancel->click();
    QVERIFY(!popup->isVisible()); QCOMPARE(changes.size(), 0);
    button->click(); QCOMPARE(models->currentData().toString(), QString("model-one"));
    QCOMPARE(efforts->currentData().toString(), QString("xhigh"));
    models->setCurrentIndex(models->findData("model-two")); QTest::keyClick(efforts, Qt::Key_Escape);
    QVERIFY(!popup->isVisible()); QCOMPARE(changes.size(), 0);
    button->click(); QCOMPARE(models->currentData().toString(), QString("model-one"));
    models->setCurrentIndex(models->findData("model-two"));
    QCOMPARE(efforts->count(), 2); QVERIFY(efforts->findData("xhigh") < 0);
    efforts->setCurrentIndex(efforts->findData("high")); QVERIFY(apply->isEnabled()); QCOMPARE(apply->text(), QString("Save for resume"));
    apply->click(); QCOMPARE(changes.size(), 1); QCOMPARE(changes[0], QVariantList({"arch/session", "model-two", "high"}));
    QCOMPARE(composer.editor()->toPlainText(), QString("[File #1] Keep this draft")); QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 1);
    composer.setModelSettings("model-one", "xhigh", options, true, {}, "model-two", "high", "ready");
    QVERIFY(button->text().contains("model-one")); QVERIFY(button->toolTip().contains("Pending: model-two  high"));
    button->click(); QCOMPARE(models->currentData().toString(), QString("model-two")); QCOMPARE(efforts->currentData().toString(), QString("high"));
    QVERIFY(composer.findChild<QLabel *>("sessionSettingsHint")->text().contains("Messages sent while it is working use the current model"));
    composer.setSessionKey("mac/other"); QVERIFY(!composer.findChild<QFrame *>("sessionSettingsPopup")->isVisible());
    apply->click(); QCOMPARE(changes.size(), 1);
    composer.setSessionKey("arch/session"); QCOMPARE(composer.editor()->toPlainText(), QString("[File #1] Keep this draft"));
}

void TestMessageComposer::modelSettingsUnavailableExplainsWhy()
{
    MessageComposer composer; composer.setSessionKey("arch/session"); composer.resize(440, 220); composer.show();
    composer.setModelSettings("observed-model", "high", {}, false, "Machine is unavailable");
    auto *button = composer.findChild<QPushButton *>("sessionModelSettings"); QVERIFY(button->isEnabled()); button->click();
    QVERIFY(composer.findChild<QFrame *>("sessionSettingsPopup")->isVisible());
    QVERIFY(composer.findChild<QLabel *>("sessionSettingsHint")->text().contains("Machine is unavailable"));
    QVERIFY(!composer.findChild<QComboBox *>("sessionModelChoice")->isEnabled());
    QVERIFY(!composer.findChild<QPushButton *>("applySessionSettings")->isEnabled());
}

void TestMessageComposer::modelSettingsFitNarrowColumn()
{
    MessageComposer composer; composer.setSessionKey("arch/session");
    composer.setModelSettings("a-very-long-model-name-in-the-catalog", "xhigh", {}, false, "Unavailable");
    composer.resize(280, 220); composer.show(); QTest::qWait(20);
    const auto *button = composer.findChild<QPushButton *>("sessionModelSettings");
    const auto *attach = composer.findChild<QPushButton *>("attachMessageFile");
    const auto *send = composer.findChild<QPushButton *>("sendMessage");
    QCOMPARE(composer.width(), 280); QVERIFY(attach->geometry().right() < button->geometry().left());
    QVERIFY(button->geometry().right() < send->geometry().left()); QVERIFY(send->geometry().right() < composer.width());
    QVERIFY(status(composer)->geometry().top() > button->geometry().bottom());
    composer.deliveryFinished("arch/session", false, "Check terminal before retrying", true); QTest::qWait(20);
    const auto *retry = composer.findChild<QPushButton *>("allowMessageRetry"); QVERIFY(retry->isVisible());
    QCOMPARE(composer.width(), 280); QVERIFY(retry->geometry().top() > button->geometry().bottom());
    QVERIFY(retry->geometry().right() < composer.width()); QVERIFY(send->geometry().right() < composer.width());
}

void TestMessageComposer::isolatesDraftsAcrossSessions()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setAvailability(true);
    composer.setSessionKey("arch\ncodex/project/first");
    composer.editor()->setPlainText("First session draft");
    QVERIFY(composer.addAttachment("first.txt", "text/plain", "first-file"));
    composer.setSessionKey("mac\nclaude/project/second");
    QVERIFY(composer.editor()->toPlainText().isEmpty());
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 0);
    composer.editor()->setPlainText("Second session draft");
    QVERIFY(composer.addAttachment("second.txt", "text/plain", "second-file"));
    composer.setSessionKey("arch\ncodex/project/first");
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] First session draft");
    sendButton(composer)->click();
    QCOMPARE(submissions.size(), 1); QCOMPARE(submissions.last().key, "arch\ncodex/project/first");
    QCOMPARE(submissions.last().attachments.size(), 1); QCOMPARE(submissions.last().attachments[0].data, "first-file");
    composer.setSessionKey("mac\nclaude/project/second");
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] Second session draft");
    sendButton(composer)->click();
    QCOMPARE(submissions.last().attachments[0].name, "second.txt");
    composer.setSessionKey({});
    QVERIFY(!composer.editor()->isEnabled()); QVERIFY(!sendButton(composer)->isEnabled());
}

void TestMessageComposer::enterSendsShiftEnterAddsLine()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch/one"); composer.setAvailability(true); composer.show();
    auto *editor = composer.editor(); editor->setFocus();
    QTest::keyClicks(editor, "First line");
    QTest::keyClick(editor, Qt::Key_Return, Qt::ShiftModifier);
    QTest::keyClicks(editor, "Second line");
    QCOMPARE(submissions.size(), 0); QCOMPARE(editor->toPlainText(), "First line\nSecond line");
    QTest::keyClick(editor, Qt::Key_Return);
    QCOMPARE(submissions.size(), 1); QCOMPARE(submissions.last().text, "First line\nSecond line");
    // The owner confirms/locks delivery; emitting a request never drops a draft.
    QCOMPARE(editor->toPlainText(), "First line\nSecond line");
}

void TestMessageComposer::inputMethodDoesNotAccidentallySubmit()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch/one"); composer.setAvailability(true); composer.show();
    auto *editor = composer.editor(); editor->setPlainText("Draft "); editor->setFocus();
    QInputMethodEvent preedit(QString::fromUtf8("に"), {});
    QApplication::sendEvent(editor, &preedit);
    QTest::keyClick(editor, Qt::Key_Return);
    QCOMPARE(submissions.size(), 0);
    QInputMethodEvent commit; commit.setCommitString(QString::fromUtf8("に"));
    QApplication::sendEvent(editor, &commit);
    QTest::keyClick(editor, Qt::Key_Return);
    QCOMPARE(submissions.size(), 1);
}

void TestMessageComposer::availabilityAndBusyGateSubmission()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch/one"); composer.show();
    composer.setAvailability(false, "Agent needs approval; use the terminal.");
    composer.editor()->setPlainText("A saved draft");
    QVERIFY(composer.addAttachment("existing.txt", "text/plain", "saved"));
    QVERIFY(composer.editor()->isEnabled()); QVERIFY(!sendButton(composer)->isEnabled());
    QTest::keyClick(composer.editor(), Qt::Key_Return); QCOMPARE(submissions.size(), 0);
    QCOMPARE(status(composer)->text(), "Agent needs approval; use the terminal.");
    QCOMPARE(QToolTip::text(), QString("Agent needs approval; use the terminal."));
    QCOMPARE(composer.editor()->toPlainText(), QString("[File #1] A saved draft"));
    composer.setAvailability(true);
    QVERIFY(sendButton(composer)->isEnabled());
    composer.setSending("arch/one");
    QVERIFY(composer.isSending("arch/one")); QVERIFY(!composer.editor()->isEnabled());
    QVERIFY(!sendButton(composer)->isEnabled());
    QVERIFY(!composer.findChild<QPushButton *>("attachMessageFile")->isEnabled());
    QVERIFY(composer.editor()->toPlainText().isEmpty());
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 0);
    QVERIFY(!composer.addAttachment("late.txt", "text/plain", "late"));
    sendButton(composer)->click(); QCOMPARE(submissions.size(), 0);
    composer.deliveryFinished("arch/one", false, "Agent was busy.");
    QVERIFY(composer.editor()->isEnabled()); QVERIFY(sendButton(composer)->isEnabled());
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] A saved draft");
}

void TestMessageComposer::syntheticImageAndFileDrop()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch/one"); composer.setAvailability(true); composer.resize(640, 250); composer.show();
    QTest::qWait(10);
    QImage image(32, 24, QImage::Format_ARGB32); image.fill(QColor("#8bdfc0"));
    QMimeData imageMime; imageMime.setImageData(image); drop(composer.editor(), imageMime);
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 1);
    QTemporaryDir directory; QVERIFY(directory.isValid());
    const QString path = directory.filePath("sample file.txt"); QFile file(path);
    QVERIFY(file.open(QIODevice::WriteOnly)); QCOMPARE(file.write("file payload"), 12); file.close();
    QMimeData fileMime; fileMime.setUrls({QUrl::fromLocalFile(path)}); drop(composer.editor(), fileMime);
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 2);
    QMimeData textMime; textMime.setText("Please inspect these attachments"); drop(composer.editor(), textMime);
    sendButton(composer)->click();
    QCOMPARE(submissions.size(), 1); QVERIFY(submissions[0].text.contains("Please inspect these attachments"));
    QVERIFY(submissions[0].text.contains("[Image #1]")); QVERIFY(submissions[0].text.contains("[File #2]"));
    QCOMPARE(submissions[0].attachments.size(), 2);
    QCOMPARE(submissions[0].attachments[0].name, "pasted-image.png");
    QCOMPARE(submissions[0].attachments[0].mime, "image/png");
    QVERIFY(submissions[0].attachments[0].data.startsWith("\x89PNG\r\n\x1a\n"));
    QCOMPARE(QImage::fromData(submissions[0].attachments[0].data).size(), image.size());
    QCOMPARE(submissions[0].attachments[1].name, "sample file.txt");
    QCOMPARE(submissions[0].attachments[1].data, "file payload");
    composer.findChildren<QPushButton *>("removeAttachment").first()->click();
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 1);
}

void TestMessageComposer::attachmentLimits()
{
    MessageComposer composer; composer.setSessionKey("count"); composer.setAvailability(true);
    for (int i = 0; i < 8; ++i) QVERIFY(composer.addAttachment(QString("file-%1.txt").arg(i), "text/plain", "small"));
    QVERIFY(!composer.addAttachment("ninth.txt", "text/plain", "small"));
    QVERIFY(status(composer)->text().contains("8 files"));
    composer.setSessionKey("size");
    QVERIFY(!composer.addAttachment("empty.txt", "text/plain", {}));
    QVERIFY(status(composer)->text().contains("empty"));
    QVERIFY(!sendButton(composer)->isEnabled());
    QVERIFY(!composer.addAttachment("large.bin", "application/octet-stream", QByteArray(10 * 1024 * 1024 + 1, 'x')));
    QVERIFY(status(composer)->text().contains("10 MiB"));
    const QByteArray maxFile(10 * 1024 * 1024, 'x');
    QVERIFY(composer.addAttachment("a.bin", "application/octet-stream", maxFile));
    QVERIFY(composer.addAttachment("b.bin", "application/octet-stream", maxFile));
    QVERIFY(!composer.addAttachment("overflow.txt", "text/plain", "x"));
    QVERIFY(status(composer)->text().contains("20 MiB"));
    composer.setSessionKey("pixels"); composer.show();
    // Monochrome storage keeps this boundary test small in memory.
    QImage oversized(8000, 4001, QImage::Format_Mono); oversized.fill(0);
    QMimeData mime; mime.setImageData(oversized); drop(composer.editor(), mime);
    QVERIFY(status(composer)->text().contains("32 megapixels"));
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 0);
}

void TestMessageComposer::textLimitCountsUtf8Bytes()
{
    MessageComposer composer; composer.setSessionKey("arch/one"); composer.setAvailability(true);
    composer.editor()->setPlainText(QString(32768, QChar(0x044f)));
    QVERIFY(sendButton(composer)->isEnabled()); // exactly 64 KiB in UTF-8
    composer.editor()->insertPlainText(QString(QChar(0x044f)));
    QVERIFY(!sendButton(composer)->isEnabled()); QVERIFY(status(composer)->text().contains("64 KiB"));
    composer.editor()->setPlainText(" \n\t"); QVERIFY(!sendButton(composer)->isEnabled());
    QVERIFY(composer.addAttachment("only.txt", "text/plain", "attachment without text"));
    QVERIFY(sendButton(composer)->isEnabled());
}

void TestMessageComposer::failureAndUncertaintyPreserveDraft()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch/one"); composer.setAvailability(true); composer.show();
    composer.editor()->setPlainText("Keep this draft"); composer.addAttachment("keep.txt", "text/plain", "keep");
    composer.setSending("arch/one"); composer.deliveryFinished("arch/one", false, "Rejected before sending");
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] Keep this draft");
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 1);
    QVERIFY(sendButton(composer)->isEnabled());
    composer.setSending("arch/one"); composer.deliveryFinished("arch/one", false, "Delivery uncertain. Check terminal.", true);
    auto *retry = composer.findChild<QPushButton *>("allowMessageRetry");
    QVERIFY(retry->isVisible()); QVERIFY(!sendButton(composer)->isEnabled());
    QTest::keyClick(composer.editor(), Qt::Key_Return); QCOMPARE(submissions.size(), 0);
    QVERIFY(composer.addAttachment("another.txt", "text/plain", "additional context"));
    QCOMPARE(status(composer)->text(), "Delivery uncertain. Check terminal.");
    QVERIFY(!sendButton(composer)->isEnabled());
    composer.setSessionKey("mac/other"); composer.setSessionKey("arch/one");
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] [File #2] Keep this draft");
    QVERIFY(retry->isVisible()); QVERIFY(!sendButton(composer)->isEnabled());
    retry->click(); QVERIFY(sendButton(composer)->isEnabled());
    sendButton(composer)->click(); QCOMPARE(submissions.size(), 1);
    QCOMPARE(submissions[0].attachments[0].data, "keep");
}

void TestMessageComposer::sendingHidesPayloadAndRestoresFailedBackgroundDelivery()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    connect(&composer, &MessageComposer::sendRequested, &composer,
        [&](const QString &key) { composer.setSending(key); });
    composer.setSessionKey("arch/one"); composer.setAvailability(true);
    composer.editor()->setPlainText("Message with attachment");
    QVERIFY(composer.addAttachment("notes.txt", "text/plain", "retained contents"));
    sendButton(composer)->click();
    QCOMPARE(submissions.size(), 1); QCOMPARE(submissions[0].text, QString("[File #1] Message with attachment"));
    QCOMPARE(submissions[0].attachments[0].data, QByteArray("retained contents"));
    QVERIFY(composer.isSending("arch/one")); QVERIFY(composer.editor()->toPlainText().isEmpty());
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 0);
    // Revisiting while delivery is pending must not redisplay the outgoing text.
    composer.setSessionKey("mac/two"); composer.editor()->setPlainText("Separate draft");
    composer.setSessionKey("arch/one"); QVERIFY(composer.editor()->toPlainText().isEmpty());
    composer.setSessionKey("mac/two");
    composer.deliveryFinished("arch/one", false, "Connection failed");
    QCOMPARE(composer.editor()->toPlainText(), QString("Separate draft"));
    composer.setSessionKey("arch/one");
    QCOMPARE(composer.editor()->toPlainText(), QString("[File #1] Message with attachment"));
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 1);
    sendButton(composer)->click(); QCOMPARE(submissions.size(), 2);
    QCOMPARE(submissions[1].attachments[0].data, QByteArray("retained contents"));
    QVERIFY(composer.editor()->toPlainText().isEmpty());
    composer.deliveryFinished("arch/one", true);
    QVERIFY(composer.editor()->toPlainText().isEmpty());
}

void TestMessageComposer::backgroundSuccessDoesNotClearVisibleDraft()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch/one"); composer.setAvailability(true);
    composer.editor()->setPlainText("Sending first"); composer.addAttachment("first.txt", "text/plain", "first");
    composer.setSending("arch/one");
    composer.setSessionKey("mac/two"); composer.editor()->setPlainText("Still editing second");
    composer.addAttachment("second.txt", "text/plain", "second");
    composer.deliveryFinished("arch/one", true);
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] Still editing second");
    sendButton(composer)->click(); QCOMPARE(submissions.size(), 1);
    QCOMPARE(submissions[0].key, "mac/two"); QCOMPARE(submissions[0].attachments[0].data, "second");
    composer.setSessionKey("arch/one"); QVERIFY(composer.editor()->toPlainText().isEmpty());
    QCOMPARE(composer.findChildren<QPushButton *>("removeAttachment").size(), 0);
    QVERIFY(!composer.isSending("arch/one"));
}

void TestMessageComposer::renamePreservesDraft()
{
    MessageComposer composer; QList<Submission> submissions; observe(composer, submissions);
    composer.setSessionKey("arch\ncodex/project/old"); composer.setAvailability(true);
    composer.editor()->setPlainText("Rename retains this draft"); composer.addAttachment("draft.txt", "text/plain", "draft");
    composer.renameDraft("arch\ncodex/project/old", "arch\ncodex/project/new");
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] Rename retains this draft");
    sendButton(composer)->click(); QCOMPARE(submissions.size(), 1);
    QCOMPARE(submissions[0].key, "arch\ncodex/project/new");
    QCOMPARE(submissions[0].attachments[0].data, "draft");
    composer.setSessionKey("mac/other");
    composer.renameDraft("arch\ncodex/project/new", "arch\ncodex/project/final");
    QVERIFY(composer.editor()->toPlainText().isEmpty());
    composer.setSessionKey("arch\ncodex/project/final");
    QCOMPARE(composer.editor()->toPlainText(), "[File #1] Rename retains this draft");
}

void TestMessageComposer::draftStateFollowsUnsentContent()
{
    // The session list marks sessions whose composer still holds an unsent message.
    MessageComposer composer; composer.setAvailability(true);
    QSignalSpy changed(&composer, &MessageComposer::draftChanged);
    composer.setSessionKey("arch/one");
    QVERIFY(!composer.hasDraft("arch/one"));
    composer.editor()->setPlainText("   \n");
    QVERIFY(!composer.hasDraft("arch/one")); QCOMPARE(changed.size(), 0);
    composer.editor()->setPlainText("Ask about the tests");
    QVERIFY(composer.hasDraft("arch/one"));
    QCOMPARE(changed.size(), 1); QCOMPARE(changed.last().at(0).toString(), QString("arch/one"));
    composer.editor()->setPlainText("Ask about the tests again");
    QCOMPARE(changed.size(), 1);   // only a change between empty and unsent is reported
    composer.setSessionKey("arch/two");
    QVERIFY(composer.hasDraft("arch/one")); QVERIFY(!composer.hasDraft("arch/two"));
    composer.setSending("arch/one");
    QVERIFY(!composer.hasDraft("arch/one")); QCOMPARE(changed.size(), 2);
    composer.deliveryFinished("arch/one", false, "Rejected before sending");
    QVERIFY(composer.hasDraft("arch/one")); QCOMPARE(changed.size(), 3);   // failed delivery is still unsent
    composer.setSending("arch/one"); composer.deliveryFinished("arch/one", true);
    QVERIFY(!composer.hasDraft("arch/one"));
    QVERIFY(composer.addAttachment("note.txt", "text/plain", "context"));
    QVERIFY(composer.hasDraft("arch/two"));
    composer.renameDraft("arch/two", "arch/renamed");
    QVERIFY(!composer.hasDraft("arch/two")); QVERIFY(composer.hasDraft("arch/renamed"));
    QVERIFY(composer.offerDraft("arch/offered", "Continue the interrupted turn"));
    QVERIFY(composer.hasDraft("arch/offered"));
}

QTEST_MAIN(TestMessageComposer)
#include "test_messagecomposer.moc"
