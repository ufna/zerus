#include "ActivityView.h"
#include "ContentScale.h"
#include "MarkdownObjects.h"
#include "SessionFileReference.h"
#include "SessionUsage.h"
#include "WorkspaceFocus.h"

#include <QClipboard>
#include <QDir>
#include <QFontInfo>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QProgressBar>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalSpy>
#include <QTextBlock>
#include <QTextBrowser>
#include <QTextCursor>
#include <QTextFragment>
#include <QTest>
#include <QVBoxLayout>

namespace {
QJsonObject journalEvent(int seq, const QString &type, const QString &detail, const QString &tool = {})
{
    return {{"seq", seq}, {"at", 1791018000 + seq}, {"type", type}, {"detail", detail}, {"tool", tool}};
}

QJsonArray history(int count)
{
    QJsonArray events;
    for (int i = 1; i <= count; ++i)
        events.append(journalEvent(i, i % 2 ? "UserPromptSubmit" : "Stop",
            QString("Message number %1\n\nReadable **formatted prose** with `inline code`.\n\nA second paragraph keeps the timeline long enough to scroll.").arg(i)));
    return events;
}

QStringList links(QTextBrowser *browser)
{
    QStringList result;
    for (auto block = browser->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            if (!format.anchorHref().isEmpty()) result.append(format.anchorHref());
        }
    return result;
}

int fontPixels(QTextBrowser *browser, const QString &text)
{
    for (auto block = browser->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it)
            if (it.fragment().text().contains(text)) return QFontInfo(it.fragment().charFormat().font()).pixelSize();
    return -1;
}

void activate(QTextBrowser *browser, const QString &url)
{
    QMetaObject::invokeMethod(browser, "anchorClicked", Qt::DirectConnection, Q_ARG(QUrl, QUrl(url)));
}
}

class TestActivityView : public QObject {
    Q_OBJECT
private slots:
    void caughtUpActivityAcknowledgesEarlierReplyWithoutIntermediatePaint();
    void updatesRemainPaintableBeforeTheNextEventLoop();
    void startsAtLatestAndFollows();
    void jumpToLatestKeepsFocusInActivity_data();
    void jumpToLatestKeepsFocusInActivity();
    void jumpOverlayPreservesViewportAndReadingPosition();
    void preservesSelectionAndReadingPosition();
    void groupsToolsAndKeepsExpansion();
    void rejectsMarkupResourcesAndUnsafeLinks();
    void preservesLiteralUserMessages_data();
    void preservesLiteralUserMessages();
    void nativeQuestionReplies_data();
    void nativeQuestionReplies();
    void unknownQuestionReplyMessages_data();
    void unknownQuestionReplyMessages();
    void nativeQuestionRepliesPreserveLiteralText();
    void preservesSessionFileReferences();
    void expandsMatchingSnapshotWithoutDuplicate();
    void kimiTurnStartedKeepsRequestBeforeTools();
    void taskNotificationIsAColoredNoticeNotYou();
    void kimiWireAnswerReplacesEmptyStop();
    void progressCommentaryAppearsBeforeFinalResponse();
    void claudeThinkingStaysSeparateFromToolsAndReplies();
    void kimiCommentaryDoesNotReplaceEmptyStop();
    void emptyPromptHooksDoNotCreateCards();
    void providerErrorsStayVisibleAndStable();
    void attachmentsStayWithTheirMessageAndOpenExactFile();
    void attachmentThumbnailArrivesWithoutMovingTranscript();
    void liveCompactionProgressPreservesTranscriptAndClears();
    void confirmedQuestionAnswerStaysInTimeline();
    void unmatchedContextPrecedesTimeline();
    void localDeliveryCards();
    void nativeQueueStaysSeparateFromHistoryAndTracksIdentity();
    void attachmentDeliveryKeepsThumbnailAndViewport();
    void contextCounterKeepsPhysicalRightAlignment();
    void failedMessageActionsFollowCardIdentity();
    void searchResultIsolatedFromLiveUpdatesAndReturnsToLatest();
    void searchResultClearsOnConversationChange();
    void searchToolExcerptIsExpandedAndClearlyMarked();
    void contentScaleScalesMarkupOnly();
    void contentScaleEnlargesTranscriptAndQueue();
    void agentMarkdownLooksLikeGitHub();
    void searchResultKeepsInlineCodeSearchable();
    void copyGivesVisibleText();
    void markdownFollowsThemeAndScale();
    void preview();
};

void TestActivityView::contentScaleScalesMarkupOnly()
{
    const QString source = "<html><head><style>body{font-size:13px;}p{margin:6px 0;line-height:135%;}</style></head><body>"
        "<p style='font-size:10px;margin:0px;'>style='font-size:12px' 14px</p>"
        "<table width='100%' cellspacing='0' cellpadding='10' style='border:1px solid #333;'><tr><td width='3'></td>"
        "<td><a href='https://example.com/icon-16px.png' title='width=\"4\" 18px'>link</a>"
        "<span style=\" font-family:'monospace'; margin-left:9px;\">code 20px</span></td></tr></table>"
        "<img src='hgs-thumbnail:key' width='96' height='64'></body></html>";
    QCOMPARE(ContentScale::html(source, 1.0), source);
    QCOMPARE(ContentScale::html(source, 2.0), QString("<html><head><style>body{font-size:26px;}p{margin:12px 0;line-height:135%;}</style></head><body>"
        "<p style='font-size:20px;margin:0px;'>style='font-size:12px' 14px</p>"
        "<table width='100%' cellspacing='0' cellpadding='20' style='border:2px solid #333;'><tr><td width='6'></td>"
        "<td><a href='https://example.com/icon-16px.png' title='width=\"4\" 18px'>link</a>"
        "<span style=\" font-family:'monospace'; margin-left:18px;\">code 20px</span></td></tr></table>"
        "<img src='hgs-thumbnail:key' width='192' height='128'></body></html>"));
    QCOMPARE(ContentScale::clamp(0.5), 1.0);
    QCOMPARE(ContentScale::clamp(4.0), 2.5);
}

void TestActivityView::contentScaleEnlargesTranscriptAndQueue()
{
    // The workspace sets this base font on every widget.
    QWidget workspace; workspace.setStyleSheet("QWidget { font-size:13px; }");
    auto *layout = new QVBoxLayout(&workspace); auto *view = new ActivityView; layout->addWidget(view);
    view->setTheme(false); workspace.resize(520, 700); workspace.show();
    view->setSessionKey("arch/scaled");
    const QString literal = "style='font-size:12px' and https://example.com/icon-16px.png";
    QJsonObject details{{"input_queue", QJsonObject{{"id", "queue"}, {"text", "queued words"}}}};
    const QJsonArray events{journalEvent(1, "UserPromptSubmit", literal),
        journalEvent(2, "Stop", "# Plan heading\n\nAgent body with [docs](https://example.com/icon-16px.png).")};
    view->setActivity(details, events);
    auto *browser = view->browser(); auto *queue = view->findChild<QLabel *>("queueText"); QVERIFY(queue);
    const int body = fontPixels(browser, "Agent body"), heading = fontPixels(browser, "Plan heading");
    const int queued = QFontInfo(queue->font()).pixelSize(), jump = QFontInfo(view->jumpButton()->font()).pixelSize();
    QCOMPARE(body, 14);
    const QSize documentSize = browser->document()->size().toSize();
    view->setContentScale(2.0);
    QCOMPARE(fontPixels(browser, "Agent body"), 28);
    QVERIFY2(fontPixels(browser, "Plan heading") >= 2 * heading - 1, qPrintable(QString::number(fontPixels(browser, "Plan heading"))));
    QCOMPARE(QFontInfo(queue->font()).pixelSize(), 26);
    QCOMPARE(QFontInfo(view->jumpButton()->font()).pixelSize(), jump);
    QVERIFY(browser->document()->size().height() > documentSize.height() * 1.5);
    // Literal text and link destinations are content, not markup.
    QVERIFY(browser->toPlainText().contains(literal));
    QVERIFY(links(browser).contains("https://example.com/icon-16px.png"));
    view->setActivity(details, events);
    QCOMPARE(fontPixels(browser, "Agent body"), 28);
    view->setContentScale(1.0);
    QCOMPARE(fontPixels(browser, "Agent body"), body);
    QCOMPARE(fontPixels(browser, "Plan heading"), heading);
    QCOMPARE(QFontInfo(queue->font()).pixelSize(), queued);
    QCOMPARE(browser->document()->size().toSize(), documentSize);
}

void TestActivityView::agentMarkdownLooksLikeGitHub()
{
    ActivityView view; view.resize(560, 600); view.show();
    view.setActivity({}, {journalEvent(1, "Stop", "Use `ctest` and:\n\n- one\n- two\n\n```\ncode\n```")});
    auto *document = view.browser()->document();
    int chips = 0; QStringList images;
    for (auto block = document->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            chips += format.objectType() == MarkdownObjects::ChipObjectType;
            if (format.isImageFormat()) images.append(format.toImageFormat().name());
        }
    QCOMPARE(chips, 1);
    QVERIFY(images.contains("hgs-md:disc/light/100")); QVERIFY(images.contains("hgs-md:corner-tl/light/100"));
    for (const auto &name : images) QVERIFY(!document->resource(QTextDocument::ImageResource, QUrl(name)).value<QImage>().isNull());
    QVERIFY(view.plainText().contains(QString::fromUtf8("Use ctest and:\n• one\n• two\ncode")));
}

void TestActivityView::searchResultKeepsInlineCodeSearchable()
{
    ActivityView view; view.resize(560, 400); view.show();
    view.showSearchResult(journalEvent(1, "AgentMessage", "Run `ctest -R markdown` now"), "ctest");
    for (auto block = view.browser()->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(it.fragment().charFormat().objectType() != MarkdownObjects::ChipObjectType);
    QVERIFY(!view.browser()->extraSelections().isEmpty());
}

void TestActivityView::copyGivesVisibleText()
{
    ActivityView view; view.resize(560, 400); view.show();
    view.setActivity({}, {journalEvent(1, "Stop", "Edit `src/cli.rs`:\n\n1. first\n2. second")});
    view.browser()->selectAll(); view.browser()->copy();
    const auto copied = QGuiApplication::clipboard()->text();
    QVERIFY2(copied.contains("Edit src/cli.rs:\n1. first\n2. second"), qPrintable(copied));
    QVERIFY(!copied.contains(QChar::ObjectReplacementCharacter));
}

void TestActivityView::markdownFollowsThemeAndScale()
{
    ActivityView view; view.resize(560, 400); view.show();
    view.setActivity({}, {journalEvent(1, "Stop", "- item with `code`")});
    view.setTheme(true); view.setContentScale(2.0);
    QStringList images; QList<QTextCharFormat> chips;
    for (auto block = view.browser()->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto format = it.fragment().charFormat();
            if (format.isImageFormat()) images.append(format.toImageFormat().name());
            if (format.objectType() == MarkdownObjects::ChipObjectType) chips.append(format);
        }
    QVERIFY(images.contains("hgs-md:disc/dark/200"));
    QCOMPARE(chips.size(), 1);
    QCOMPARE(chips.first().property(QTextFormat::UserProperty + 45).toDouble(), 2.0);
    const auto disc = view.browser()->document()->resource(QTextDocument::ImageResource, QUrl("hgs-md:disc/dark/200")).value<QImage>();
    QCOMPARE(disc.size(), (QSizeF(10, 38) * view.browser()->devicePixelRatioF()).toSize());
}

void TestActivityView::contextCounterKeepsPhysicalRightAlignment()
{
    SessionUsage::ContextButton button; button.setTheme(false);
    button.setData({{"context",QJsonObject{{"used",1500},{"limit",10000}}}});
    button.show(); button.clearFocus(); QTest::qWait(10);
    const auto image=button.grab().toImage(); int rightmost=-1;
    for(int y=0;y<image.height();++y)for(int x=0;x<image.width();++x) {
        const auto pixel=image.pixelColor(x,y);
        if(pixel.green()>pixel.red()+15 && pixel.green()>pixel.blue()+10) rightmost=qMax(rightmost,x);
    }
    QVERIFY2(rightmost>=image.width()-qCeil(12*image.devicePixelRatio()),"Context text must end at the physical right edge");
}

void TestActivityView::attachmentDeliveryKeepsThumbnailAndViewport()
{
    ActivityView view; view.resize(540,380); view.show(); view.setSessionKey("arch/session");
    const auto events = history(30);
    QJsonObject details{{"conversation_id","conversation"}};
    QJsonObject file{{"name","screenshot.png"},{"mime","image/png"},{"bytes",120},
        {"reference","[Image #1]"},{"local_path","/cached/screenshot.png"}};
    QJsonObject local{{"id","outgoing-1"},{"text","[Image #1] Review this image"},{"status","sending"},
        {"submitted_at",1791018100},{"attachments",QJsonArray{file}}};
    QSignalSpy previews(&view,&ActivityView::attachmentPreviewRequested);
    view.setTimeline(details,events,{local}); QTRY_COMPARE(previews.size(),1);
    const QString key=previews[0][0].toString();
    QImage thumbnail(192,128,QImage::Format_ARGB32); thumbnail.fill(Qt::red);
    view.setAttachmentPreview(key,thumbnail);
    auto *browser=view.browser(); auto *bar=browser->verticalScrollBar();
    QTRY_COMPARE(bar->value(),bar->maximum());
    const auto top=browser->cursorForPosition(QPoint(2,2));
    const auto topText=top.block().text(); const auto topY=browser->cursorRect(top).top();
    // Read the sent card while its acknowledgement and native journal arrive.
    auto selection=browser->document()->find("Review this image");browser->setTextCursor(selection);
    local["status"]="sent";local["message_id"]="delivery-1";file["request_id"]="delivery-1";file["index"]=0;
    local["attachments"]=QJsonArray{file};
    view.setTimeline(details,events,{local});
    QCOMPARE(browser->textCursor().selectedText(),QString("Review this image"));
    file.remove("local_path");
    details["attachment_messages"]=QJsonArray{QJsonObject{{"type","UserPromptSubmit"},{"source","hgs_delivery"},
        {"message_id","delivery-1"},{"at",1791018100},{"detail",local["text"]},{"submitted_text",local["text"]},
        {"attachments",QJsonArray{file}}}};
    view.setTimeline(details,events,{});
    QCOMPARE(browser->textCursor().selectedText(),QString("Review this image"));
    auto nativeEvents=events;nativeEvents.append(QJsonObject{{"seq",31},{"at",1791018101},{"type","UserPromptSubmit"},{"detail",local["text"]}});
    view.setTimeline(details,nativeEvents,{});
    QTest::qWait(20);
    QCOMPARE(previews.size(),1);
    QCOMPARE(browser->textCursor().selectedText(),QString("Review this image"));
    const auto restoredTop=browser->cursorForPosition(QPoint(2,2));
    QCOMPARE(restoredTop.block().text(),topText);QCOMPARE(browser->cursorRect(restoredTop).top(),topY);
    QCOMPARE(browser->toPlainText().count("Review this image"),1);
    const auto resource=browser->document()->resource(QTextDocument::ImageResource,QUrl("hgs-thumbnail:"+key)).value<QImage>();
    QVERIFY(!resource.isNull());QCOMPARE(resource.pixelColor(30,30),QColor(Qt::red));
    QSignalSpy opened(&view,&ActivityView::attachmentActivated);
    activate(browser,"hgs-attachment:"+key);QCOMPARE(opened.size(),1);
    QCOMPARE(opened[0][0].toJsonObject(),file); // Opening uses the new durable reference.
}

void TestActivityView::nativeQueueStaysSeparateFromHistoryAndTracksIdentity()
{
    ActivityView view; view.resize(600,550); view.setSessionKey("queue-session"); view.show();
    auto events = history(15); QJsonObject details{{"conversation_id","conversation"}};
    view.setActivity(details,events);
    auto *send = view.findChild<QPushButton *>("queueSendNow");
    auto *panel = view.findChild<QWidget *>("activityInputQueue"); QVERIFY(!panel->isVisible());
    auto queue = QJsonObject{{"id","queue-1"},{"text","❯ first queued message"},{"can_send_now",true},{"hint","Native action"}};
    details["input_queue"] = queue; view.setActivity(details,events); QVERIFY(panel->isVisible()); QVERIFY(send->isEnabled());
    auto *browser = view.browser(); const auto revision = browser->document()->revision();
    browser->setTextCursor(browser->document()->find("Message number 1"));
    QSignalSpy sent(&view,&ActivityView::queueSendNowRequested);
    send->click(); QCOMPARE(sent.size(),1); QCOMPARE(sent[0][0].toString(),QString("queue-1"));
    queue["id"]="queue-2";queue["text"]="❯ newer queue";details["input_queue"]=queue;
    view.setActivity(details,events); QCOMPARE(browser->document()->revision(),revision);
    QCOMPARE(browser->textCursor().selectedText(),QString("Message number 1"));
    send->click(); QCOMPARE(sent[1][0].toString(),QString("queue-2"));
    details["queue_sending"]=true;view.setActivity(details,events);QVERIFY(!send->isEnabled());
    details.remove("input_queue");view.setActivity(details,events);QVERIFY(!panel->isVisible());
    QVERIFY(browser->toPlainText().contains("Sent"));
    view.setActivity({{"input_queue",queue}},events);view.setSessionKey("another");QVERIFY(!panel->isVisible());
}

void TestActivityView::updatesRemainPaintableBeforeTheNextEventLoop()
{
    class PaintCounter : public QObject {
    public:
        int paints = 0;
        bool eventFilter(QObject *, QEvent *event) override {
            if (event->type() == QEvent::Paint) ++paints;
            return false;
        }
    } counter;
    ActivityView view; view.resize(520, 340); view.show(); view.setSessionKey("arch/session");
    auto events = history(24); view.setActivity({{"conversation_id", "one"}}, events);
    QTest::qWait(30);
    auto *browser = view.browser(); browser->viewport()->installEventFilter(&counter);
    browser->verticalScrollBar()->setValue(0);
    auto selection = browser->document()->find("Message number 1"); browser->setTextCursor(selection);
    const auto selected = browser->textCursor().selectedText();
    const int scroll = browser->verticalScrollBar()->value();
    for (int i = 25; i <= 28; ++i) {
        events.append(journalEvent(i, "AgentMessage", QString("New response %1").arg(i)));
        view.setActivity({{"conversation_id", "one"}}, events);
        // Another widget can repaint the parent before queued callbacks run.
        // Activity must participate in that frame, never leave a blank rectangle.
        counter.paints = 0; view.repaint();
        QVERIFY2(counter.paints > 0, "Activity disappeared from the parent repaint while its update was pending");
        QCOMPARE(browser->textCursor().selectedText(), selected);
        QCOMPARE(browser->verticalScrollBar()->value(), scroll);
        QVERIFY(browser->toPlainText().contains(QString("New response %1").arg(i)));
    }
}

void TestActivityView::jumpOverlayPreservesViewportAndReadingPosition()
{
    ActivityView view;view.resize(700,420);view.show();view.setSessionKey("overlay");
    auto events=history(24);view.setActivity({},events);
    auto *browser=view.browser();auto *bar=browser->verticalScrollBar();
    auto *latest=view.findChild<QPushButton *>("activityJumpLatest");
    QTRY_VERIFY(bar->maximum()>0);QTRY_COMPARE(bar->value(),bar->maximum());
    const auto viewport=browser->viewport()->geometry();
    bar->setValue(bar->maximum()/2);QTRY_VERIFY(latest->isVisible());
    QCOMPARE(browser->viewport()->geometry(),viewport);
    auto cursor=browser->cursorForPosition(QPoint(8,8));const int position=cursor.position();const auto reading=browser->cursorRect(cursor).top();
    events.append(journalEvent(100,"AgentMessage","One new reply"));view.setActivity({},events);QTest::qWait(20);
    cursor=QTextCursor(browser->document());cursor.setPosition(position);
    QCOMPARE(browser->viewport()->geometry(),viewport);QCOMPARE(browser->cursorRect(cursor).top(),reading);
    QVERIFY(latest->text().contains("1 new"));
    for (const auto size:{QSize(430,310),QSize(820,500)}) {
        view.resize(size);QTest::qWait(20);
        const auto area=QRect(browser->viewport()->mapTo(&view,QPoint()),browser->viewport()->size());
        const auto button=QRect(latest->mapTo(&view,QPoint()),latest->size());
        QVERIFY(area.contains(button));QVERIFY(qAbs(area.center().x()-button.center().x())<=1);
        QCOMPARE(area.bottom()-button.bottom(),8);
    }
    const auto preview=qEnvironmentVariable("HGS_JUMP_PREVIEW");if(!preview.isEmpty()){view.setTheme(true);QTest::qWait(20);QVERIFY(view.grab().save(preview));}
    const auto beforeJump=browser->viewport()->geometry();latest->click();QTest::qWait(20);
    QVERIFY(latest->isHidden());QCOMPARE(browser->viewport()->geometry(),beforeJump);QCOMPARE(bar->value(),bar->maximum());
}

void TestActivityView::jumpToLatestKeepsFocusInActivity_data()
{
    QTest::addColumn<bool>("keyboard");
    QTest::addColumn<bool>("search");
    QTest::newRow("mouse") << false << false;
    QTest::newRow("keyboard") << true << false;
    QTest::newRow("mouse-from-search") << false << true;
    QTest::newRow("keyboard-from-search") << true << true;
}

void TestActivityView::jumpToLatestKeepsFocusInActivity()
{
    QFETCH(bool, keyboard); QFETCH(bool, search);
    WorkspaceFocus::install();
    QWidget window; QVBoxLayout layout(&window);
    ActivityView view; SessionUsage::ContextButton context; QLineEdit composer;
    context.setData({{"context", QJsonObject{{"used", 220000}, {"limit", 1000000}}}});
    layout.addWidget(&view, 1); layout.addWidget(&context); layout.addWidget(&composer);
    window.resize(520, 420); window.show(); window.activateWindow();
    QTRY_VERIFY(window.isActiveWindow());
    view.setSessionKey("arch/session"); view.setActivity({}, history(24));
    auto *browser = view.browser(); auto *bar = browser->verticalScrollBar();
    auto *latest = view.findChild<QPushButton *>("activityJumpLatest");
    QTRY_VERIFY(bar->maximum() > 0); QTRY_COMPARE(bar->value(), bar->maximum());
    if (search) view.showSearchResult(journalEvent(101, "AgentMessage", "Saved reply"), "reply");
    else bar->setValue(0);
    QTRY_VERIFY(latest->isVisible());
    if (keyboard) {
        latest->setFocus(Qt::TabFocusReason);
        QVERIFY(latest->hasFocus());
        QTest::keyClick(latest, Qt::Key_Space);
    } else QTest::mouseClick(latest, Qt::LeftButton);
    QTRY_VERIFY(latest->isHidden()); QTRY_COMPARE(bar->value(), bar->maximum());
    QVERIFY(browser->hasFocus()); QVERIFY(!context.hasFocus());
    QVERIFY(!browser->textCursor().hasSelection());

    // Context remains reachable via Tab, with a keyboard-only focus ring.
    QTest::keyClick(browser, Qt::Key_Tab);
    QVERIFY(context.hasFocus()); QVERIFY(context.property("keyboardFocus").toBool());
    QTest::mouseClick(&context, Qt::LeftButton);
    QVERIFY(!context.property("keyboardFocus").toBool());

    // Automatic following must not take focus away from a message draft.
    composer.setFocus(Qt::MouseFocusReason);
    view.setActivity({}, history(26));
    QTRY_COMPARE(bar->value(), bar->maximum()); QVERIFY(composer.hasFocus());
}

void TestActivityView::liveCompactionProgressPreservesTranscriptAndClears()
{
    ActivityView view; view.resize(520, 380); view.show(); view.setSessionKey("kimi/session");
    QJsonObject details{{"runtime_state","live"},{"process_state","running"},{"phase","working"}};
    const QJsonArray events{journalEvent(1,"UserPromptSubmit","Review this project"), journalEvent(2,"Stop","Previous response")};
    view.setActivity(details, events);
    auto *row = view.findChild<QWidget *>("activityCompaction"); QVERIFY(row); QVERIFY(row->isHidden());
    const auto revision = view.browser()->document()->revision();
    auto cursor = view.browser()->document()->find("Previous response"); view.browser()->setTextCursor(cursor);
    details["phase"] = "compacting"; view.setActivity(details, events);
    QVERIFY(!row->isHidden()); QCOMPARE(view.browser()->document()->revision(), revision);
    QCOMPARE(view.browser()->textCursor().selectedText(), QString("Previous response"));
    auto *progress=view.findChild<QProgressBar *>("compactionProgress"); QVERIFY(progress);
    QCOMPARE(progress->minimum(),0); QCOMPARE(progress->maximum(),0); QVERIFY(!progress->isTextVisible());
    QCOMPARE(view.findChild<QLabel *>("compactionCaption")->text(), QString("Compacting context…"));
    const auto preview=qEnvironmentVariable("HGS_COMPACTION_PREVIEW");
    if(!preview.isEmpty()) for(bool dark:{false,true}) {view.setTheme(dark);QTest::qWait(80);QVERIFY(view.grab().save(preview+(dark?"-dark.png":"-light.png")));}
    view.showSearchResult(events[0].toObject(), "Review"); QVERIFY(row->isHidden());
    view.clearSearchResult(); QVERIFY(!row->isHidden());
    for(const auto phase:{"working","tool","idle","error"}) {details["phase"]=phase;view.setActivity(details,events);QVERIFY(row->isHidden());}
    details["phase"]="compacting";
    for(const auto runtime:{"stopped","unavailable"}) {details["runtime_state"]=runtime;view.setActivity(details,events);QVERIFY(row->isHidden());}
    details["runtime_state"]="live";details["state"]="archived";view.setActivity(details,events);QVERIFY(row->isHidden());
    details.remove("state");details["process_state"]="exited";view.setActivity(details,events);QVERIFY(row->isHidden());
    details["process_state"]="running";view.setActivity(details,events);QVERIFY(!row->isHidden());
    view.setSessionKey("other/session");QVERIFY(row->isHidden());
}

void TestActivityView::searchResultIsolatedFromLiveUpdatesAndReturnsToLatest()
{
    ActivityView view; view.resize(520, 340); view.show(); view.setSessionKey("arch/session");
    view.setActivity({{"conversation_id", "one"}, {"last_message", "Latest live response"}}, history(24));
    view.setLocalMessages({QJsonObject{{"request_id", "draft"}, {"text", "Pending local message"}, {"state", "sending"}}});
    QTest::qWait(20);
    auto saved = journalEvent(101, "AgentMessage", "Saved **NEEDLE** response from history.");
    view.showSearchResult(saved, "needle");
    auto *browser = view.browser(); auto *latest = view.findChild<QPushButton *>("activityJumpLatest");
    QVERIFY(browser->toPlainText().contains("Saved NEEDLE response"));
    QVERIFY(!browser->toPlainText().contains("Latest live response"));
    QVERIFY(!browser->toPlainText().contains("Pending local message"));
    QVERIFY(!browser->toPlainText().contains("Message number"));
    QVERIFY(!browser->extraSelections().isEmpty());
    QCOMPARE(browser->extraSelections().first().cursor.selectedText(), "NEEDLE");
    QVERIFY(latest->isVisible()); QVERIFY(latest->text().contains("Back to latest"));
    // Polling keeps the saved message stable while the live timeline advances behind it.
    const int revision = browser->document()->revision();
    view.setActivity({{"conversation_id", "one"}, {"last_message", "New live response"}}, history(26));
    QCOMPARE(browser->document()->revision(), revision);
    QVERIFY(browser->toPlainText().contains("Saved NEEDLE response"));
    latest->click();
    QVERIFY(browser->toPlainText().contains("Message number 26"));
    QVERIFY(browser->toPlainText().contains("Pending local message"));
    QVERIFY(!browser->toPlainText().contains("Saved NEEDLE response"));
    QVERIFY(browser->extraSelections().isEmpty());
    QTRY_COMPARE(browser->verticalScrollBar()->value(), browser->verticalScrollBar()->maximum());
}
void TestActivityView::searchResultClearsOnConversationChange()
{
    ActivityView view; view.resize(520, 340); view.show(); view.setSessionKey("arch/session");
    view.setActivity({{"conversation_id", "one"}}, history(12));
    view.showSearchResult(journalEvent(100, "AgentMessage", "Needle from old conversation"), "needle");
    QVERIFY(view.browser()->toPlainText().contains("old conversation"));
    view.setActivity({{"conversation_id", "two"}}, history(14));
    QVERIFY(!view.browser()->toPlainText().contains("old conversation"));
    QVERIFY(view.browser()->extraSelections().isEmpty());
    QVERIFY(view.browser()->toPlainText().contains("Message number 14"));
    QTRY_COMPARE(view.browser()->verticalScrollBar()->value(), view.browser()->verticalScrollBar()->maximum());
    view.showSearchResult(journalEvent(100, "AgentMessage", "Needle from previous session"), "needle");
    view.setSessionKey("mac/another"); view.setActivity({}, history(2));
    QVERIFY(!view.browser()->toPlainText().contains("previous session"));
    QVERIFY(view.browser()->extraSelections().isEmpty());
}
void TestActivityView::searchToolExcerptIsExpandedAndClearlyMarked()
{
    ActivityView view; view.setSessionKey("arch/session"); view.setActivity({}, history(2));
    auto event = journalEvent(99, "PostToolUse", "A long log excerpt containing needle", "Bash");
    event["content_truncated"] = true;
    view.showSearchResult(event, "needle");
    QVERIFY(view.browser()->toPlainText().contains("A long log excerpt containing needle"));
    QVERIFY(view.browser()->toPlainText().contains("Excerpt around the match"));
    QVERIFY(view.browser()->toPlainText().contains("too long to display in full"));
    QVERIFY(!view.browser()->extraSelections().isEmpty());
    view.clearSearchResult();
    QVERIFY(!view.browser()->toPlainText().contains("Excerpt around the match"));
    QVERIFY(view.browser()->extraSelections().isEmpty());
}

void TestActivityView::kimiWireAnswerReplacesEmptyStop()
{
    ActivityView view; view.resize(520, 340); view.show(); view.setSessionKey("kimi/project");
    const QJsonArray events{journalEvent(1, "TurnStarted", "Please push"),
        journalEvent(2, "PostToolUse", "git push", "Bash"), journalEvent(3, "Stop", "")};
    const auto reply = QJsonObject{{"type", "AgentMessage"}, {"source", "kimi_wire"},
        {"at", 1791018003.0}, {"detail", "All pushed. **343 tests passed.**"}};
    view.setActivity({{"provider_messages", QJsonArray{reply}}}, events);
    const auto text = view.browser()->toPlainText();
    QCOMPARE(text.count("All pushed."), 1);
    QVERIFY(text.indexOf("All pushed.") > text.indexOf("Please push"));
    QVERIFY(!text.contains("Outside the available timeline"));
    view.setActivity({{"provider_messages", QJsonArray{reply}}}, events);
    QCOMPARE(view.browser()->toPlainText().count("All pushed."), 1);
    QTRY_COMPARE(view.browser()->verticalScrollBar()->value(), view.browser()->verticalScrollBar()->maximum());
}

void TestActivityView::progressCommentaryAppearsBeforeFinalResponse()
{
    for (const QString &source : {QString("codex_transcript"),QString("claude_transcript"),QString("kimi_wire")}) {
        ActivityView view; view.setSessionKey(source);
        auto progress=journalEvent(0,"AgentMessage","Checking the deployment.");progress["at"]=1791018002.0;progress["source"]=source;
        auto final=journalEvent(0,"AgentMessage","Deployment complete.");final["at"]=1791018004.0;final["source"]=source;
        QJsonArray hooks{journalEvent(1,"UserPromptSubmit","Deploy"),journalEvent(3,"PostToolUse","Check services","Bash")};
        view.setActivity({{"provider_messages",QJsonArray{progress}}},hooks);
        QCOMPARE(view.browser()->toPlainText().count("Checking the deployment."),1);
        hooks.append(journalEvent(4,"Stop","Deployment complete."));
        view.setActivity({{"provider_messages",QJsonArray{progress,final}}},hooks);
        const auto text=view.browser()->toPlainText();
        QCOMPARE(text.count("Checking the deployment."),1);QCOMPARE(text.count("Deployment complete."),1);
        QVERIFY(text.indexOf("Checking the deployment.")<text.indexOf("Deployment complete."));
    }
}

void TestActivityView::claudeThinkingStaysSeparateFromToolsAndReplies()
{
    ActivityView view;view.setSessionKey("claude/thinking");view.resize(700,500);view.show();
    const auto body=QString("Read all 24 pages. ")+QString("More recorded thinking. ").repeated(10)+"END_OF_THINKING";
    auto thinking=journalEvent(0,"AgentThinking",body);thinking["source"]="claude_transcript";thinking["message_id"]="thinking-one";thinking["at"]=1791018002.0;
    QJsonArray hooks{journalEvent(1,"PreToolUse","Read report","Read"),journalEvent(3,"PostToolUse","Inspect source","Bash"),journalEvent(4,"Stop","")};
    const QJsonObject details{{"provider_messages",QJsonArray{thinking}}};view.setActivity(details,hooks);
    auto *browser=view.browser();const auto text=browser->toPlainText();
    QVERIFY(text.contains("Thinking"));QCOMPARE(text.count("Read all 24 pages."),1);QVERIFY(!text.contains("END_OF_THINKING"));
    QVERIFY(text.indexOf("Thinking")>text.indexOf("Read report"));QVERIFY(text.indexOf("Thinking")<text.indexOf("Bash"));
    QString toggle;
    for(auto block=browser->document()->begin();block.isValid();block=block.next())
        for(auto it=block.begin();!it.atEnd();++it)if(it.fragment().text().contains("Thinking"))toggle=it.fragment().charFormat().anchorHref();
    QVERIFY(!toggle.isEmpty());activate(browser,toggle);QVERIFY(browser->toPlainText().contains("END_OF_THINKING"));
    view.setActivity(details,hooks);QCOMPARE(browser->toPlainText().count("END_OF_THINKING"),1);
    const auto preview=qEnvironmentVariable("HGS_THINKING_PREVIEW");if(!preview.isEmpty()){QDir().mkpath(preview);QVERIFY(view.grab().save(preview+"/claude-thinking.png"));}
}

void TestActivityView::kimiCommentaryDoesNotReplaceEmptyStop()
{
    ActivityView view; view.setSessionKey("kimi/project");
    auto started = journalEvent(2, "PreToolUse", "Writing the file", "Write");
    started["at"] = 1791018002.5;
    const QJsonArray events{journalEvent(1, "TurnStarted", "Build the file"), started,
        journalEvent(3, "PostToolUse", "Wrote the file", "Write"), journalEvent(4, "Stop", "")};
    const QJsonObject progress{{"type", "AgentMessage"}, {"source", "kimi_wire"},
        {"message_id", "kimi-step:progress"}, {"message_phase", "commentary"},
        {"at", 1791018002.0}, {"detail", "Now writing the HTML file."}};
    const QJsonObject final{{"type", "AgentMessage"}, {"source", "kimi_wire"},
        {"message_id", "kimi-step:final"}, {"message_phase", "final"},
        {"at", 1791018003.9}, {"detail", "File ready."}};
    for (const QJsonArray &messages : {QJsonArray{progress}, QJsonArray{progress, final}}) {
        view.setActivity({{"provider_messages", messages}}, events);
        const auto text = view.browser()->toPlainText();
        QCOMPARE(text.count("Now writing the HTML file."), 1);
        QVERIFY(text.indexOf("Now writing") < text.indexOf("1 tool call"));
        if (messages.size() == 2) {
            QCOMPARE(text.count("File ready."), 1);
            QVERIFY(text.indexOf("1 tool call") < text.indexOf("File ready."));
            QVERIFY(!text.contains("Response finished"));
        }
    }
}

void TestActivityView::emptyPromptHooksDoNotCreateCards()
{
    ActivityView view;
    QJsonArray events{journalEvent(1, "UserPromptSubmit", ""),
        journalEvent(2, "TurnStarted", "Please inspect this"),
        journalEvent(3, "StopFailure", "Provider failed"),
        journalEvent(4, "UserPromptSubmit", " \n "),
        journalEvent(5, "TurnStarted", "Try again")};
    view.setActivity({}, events);
    auto text = view.browser()->toPlainText();
    QVERIFY(!text.contains("UserPromptSubmit"));
    QCOMPARE(text.count("Please inspect this"), 1);
    QCOMPARE(text.count("Try again"), 1);
    QVERIFY(text.contains("Response failed"));
    QVERIFY(!links(view.browser()).contains("hgs-activity:group-1"));
    QVERIFY(!links(view.browser()).contains("hgs-activity:group-4"));
    // Submission hooks with actual content still supply the user message.
    auto attachment = journalEvent(6, "UserPromptSubmit", "");
    attachment["attachments"] = QJsonArray{QJsonObject{{"name", "screen.png"}}};
    events.append(attachment);
    events.append(journalEvent(7, "UserPromptSubmit", "A regular prompt"));
    view.setActivity({}, events);
    text = view.browser()->toPlainText();
    QCOMPARE(text.count("screen.png"), 1);
    QCOMPARE(text.count("A regular prompt"), 1);
    QVERIFY(!text.contains("UserPromptSubmit"));
}

void TestActivityView::providerErrorsStayVisibleAndStable()
{
    ActivityView view;view.setSessionKey("codex/project/session");
    const QJsonObject error{{"type","StopFailure"},{"source","codex_native_log"},{"message_id","codex-error-1"},
        {"at",1791018002.5},{"detail","Selected model is at capacity. Please try a different model."}};
    const QJsonObject details{{"provider_errors",QJsonArray{error,error}}};
    QJsonArray events{journalEvent(1,"UserPromptSubmit","Start"),journalEvent(2,"PostToolUse","Success","Bash")};
    view.setActivity(details,events);
    auto text=view.browser()->toPlainText();QCOMPARE(text.count("Selected model is at capacity."),1);
    QVERIFY(text.contains("Response failed"));QVERIFY(text.indexOf("Selected model")>text.indexOf("Start"));
    const auto revision=view.browser()->document()->revision();view.setActivity(details,events);
    QCOMPARE(view.browser()->document()->revision(),revision);
    events.append(journalEvent(3,"UserPromptSubmit","Retry"));view.setActivity(details,events);
    text=view.browser()->toPlainText();QCOMPARE(text.count("Selected model is at capacity."),1);
    QVERIFY(text.indexOf("Selected model")<text.indexOf("Retry"));
    view.setSessionKey("codex/other");view.setActivity({},{});QVERIFY(!view.browser()->toPlainText().contains("Selected model"));
}

void TestActivityView::attachmentsStayWithTheirMessageAndOpenExactFile()
{
    ActivityView view;view.setSessionKey("mac/project/session");
    const QJsonObject file{{"request_id","receipt-one"},{"index",0},{"name","screen.png"},{"mime","image/png"},{"bytes",2048}};
    QJsonArray events{journalEvent(1,"Stop","Earlier response"),journalEvent(4,"Stop","Later response")};
    view.setActivity({},events);view.setLocalMessages({QJsonObject{{"id","1"},{"text",""},{"status","sent"},{"submitted_at",1791018002.0},{"attachments",QJsonArray{file}}}});
    auto text=view.browser()->toPlainText();QVERIFY(text.indexOf("Earlier response")<text.indexOf("screen.png"));QVERIFY(text.indexOf("screen.png")<text.indexOf("Later response"));
    QSignalSpy opened(&view,&ActivityView::attachmentActivated);QString link;
    for(const auto &url:links(view.browser()))if(url.startsWith("hgs-attachment:")){link=url;break;}
    QVERIFY(!link.isEmpty());activate(view.browser(),link);QCOMPARE(opened.size(),1);QCOMPARE(opened.first().first().toJsonObject(),file);
    const auto native=journalEvent(2,"UserPromptSubmit","Inspect the attached image at \"/private/image.png\"");events.insert(1,native);
    const QJsonObject receipt{{"source","hgs_delivery"},{"type","UserPromptSubmit"},{"message_id","receipt-one"},{"at",1791018001.8},
        {"detail",""},{"submitted_text","\n\nInspect the attached image at \"/private/image.png\""},{"attachments",QJsonArray{file}}};
    events.insert(2,journalEvent(3,"TurnStarted","Inspect the attached image at \"/private/image.png\""));
    view.setLocalMessages({});view.setActivity({{"attachment_messages",QJsonArray{receipt}}, {"prompt","Inspect the attached image at \"/private/image.png\""}},events);
    text=view.browser()->toPlainText();QCOMPARE(text.count("screen.png"),1);QVERIFY(!text.contains("/private/image.png"));QVERIFY(text.indexOf("screen.png")<text.indexOf("Later response"));
    view.setSessionKey("other");activate(view.browser(),link);QCOMPARE(opened.size(),1);
}

void TestActivityView::attachmentThumbnailArrivesWithoutMovingTranscript()
{
    ActivityView view;view.resize(640,420);view.show();view.setSessionKey("arch/session");
    QSignalSpy previews(&view,&ActivityView::attachmentPreviewRequested);
    const QJsonObject file{{"name","screen.png"},{"mime","image/png"},{"request_id","receipt"},{"index",0},{"bytes",38400}};
    auto events=history(12);
    auto message=journalEvent(13,"UserPromptSubmit","See this image");message["attachments"]=QJsonArray{file};events.append(message);
    view.setActivity({{"conversation_id","one"}},events);
    QTRY_COMPARE(previews.size(),1);const auto key=previews.first()[0].toString();
    auto *browser=view.browser();auto *bar=browser->verticalScrollBar();
    QTRY_COMPARE(bar->value(),bar->maximum());
    auto cursor=browser->document()->find("See this image");browser->setTextCursor(cursor);
    const auto revision=browser->document()->revision();const auto scroll=bar->value();
    QImage image(320,200,QImage::Format_RGB32);image.fill(QColor("#83cbb2"));
    view.setAttachmentPreview(key,image);QCoreApplication::processEvents();
    QCOMPARE(browser->document()->revision(),revision);QCOMPARE(bar->value(),scroll);
    QCOMPARE(browser->textCursor().selectedText(),QString("See this image"));
    const QUrl resource("hgs-thumbnail:"+key);
    QVERIFY(!browser->document()->resource(QTextDocument::ImageResource,resource).value<QImage>().isNull());
    events.append(journalEvent(14,"Stop","Image received"));view.setActivity({{"conversation_id","one"}},events);
    QCoreApplication::processEvents();QCOMPARE(previews.size(),1);
    QVERIFY(!browser->document()->resource(QTextDocument::ImageResource,resource).value<QImage>().isNull());
    const auto preview=qEnvironmentVariable("HGS_INLINE_ATTACHMENT_PREVIEW");
    if(!preview.isEmpty()){view.setTheme(true);view.jumpToLatest();QTest::qWait(40);QVERIFY(view.grab().save(preview));}
    view.setSessionKey("other/session");view.setActivity({},history(4));
    const auto otherRevision=browser->document()->revision();view.setAttachmentPreview(key,image);
    QCOMPARE(browser->document()->revision(),otherRevision);QVERIFY(!browser->toPlainText().contains("screen.png"));
}

void TestActivityView::startsAtLatestAndFollows()
{
    ActivityView view; view.resize(520, 340); view.show();
    view.setSessionKey("arch/one"); view.setActivity({}, {});
    QTest::qWait(20);
    auto *bar = view.browser()->verticalScrollBar();
    view.setActivity({}, history(24));
    QTRY_VERIFY(bar->maximum() > 0);
    QTRY_COMPARE(bar->value(), bar->maximum());
    QVERIFY(view.browser()->toPlainText().indexOf("Message number 1\n") < view.browser()->toPlainText().indexOf("Message number 24\n"));
    const int oldMaximum = bar->maximum();
    view.setActivity({}, history(26));
    QTRY_VERIFY(bar->maximum() > oldMaximum);
    QTRY_COMPARE(bar->value(), bar->maximum());
    view.resize(420, 290); QTest::qWait(20);
    QTRY_COMPARE(bar->value(), bar->maximum());
    bar->setValue(0);
    view.setSessionKey("mac/two"); view.setActivity({}, history(20));
    QTRY_COMPARE(bar->value(), bar->maximum());
    // A new conversation behind the same pane is a fresh history too.
    view.setActivity({{"conversation_id", "a"}}, history(20)); bar->setValue(0);
    view.setActivity({{"conversation_id", "b"}}, history(22));
    QTRY_COMPARE(bar->value(), bar->maximum());
}

void TestActivityView::preservesSelectionAndReadingPosition()
{
    ActivityView view; view.resize(560, 350); view.show();
    view.setSessionKey("arch/one"); view.setActivity({}, history(30));
    QTest::qWait(20);
    auto *browser = view.browser(); auto *bar = browser->verticalScrollBar();
    auto selection = browser->document()->find("Message number 12"); QVERIFY(!selection.isNull());
    browser->setTextCursor(selection);
    bar->setValue(bar->value() - 50); QTest::qWait(20);
    const auto top = browser->cursorForPosition(QPoint(4, 4));
    const QString topText = top.block().text(); const int topY = browser->cursorRect(top).top();
    const QString selected = browser->textCursor().selectedText();
    const int revision = browser->document()->revision();
    view.setActivity({}, history(30));
    QCOMPARE(browser->document()->revision(), revision); // unchanged polls never reset selection
    view.setActivity({}, history(32)); QTest::qWait(20);
    QCOMPARE(browser->textCursor().selectedText(), selected);
    const auto afterTop = browser->cursorForPosition(QPoint(4, 4));
    QCOMPARE(afterTop.block().text(), topText);
    QVERIFY(qAbs(browser->cursorRect(afterTop).top() - topY) < 3);
    QVERIFY(bar->value() < bar->maximum() - 30);
    auto *latest = view.findChild<QPushButton *>("activityJumpLatest");
    QVERIFY(latest->isVisible()); QVERIFY(latest->text().contains("2 new"));
    // Pruning old events keeps the selected event and the visible paragraph.
    auto pruned = history(33); for (int i = 0; i < 8; ++i) pruned.removeFirst();
    view.setActivity({}, pruned); QTest::qWait(20);
    QCOMPARE(browser->textCursor().selectedText(), selected);
    QCOMPARE(browser->cursorForPosition(QPoint(4, 4)).block().text(), topText);
    latest->click();
    QTRY_COMPARE(bar->value(), bar->maximum());
    QVERIFY(!latest->isVisible()); QVERIFY(!browser->textCursor().hasSelection());
}

void TestActivityView::groupsToolsAndKeepsExpansion()
{
    ActivityView view; view.resize(540, 320); view.show();
    view.setSessionKey("arch/one");
    QJsonArray events{journalEvent(1, "UserPromptSubmit", "Please inspect the source"),
        journalEvent(2, "PreToolUse", "hidden-command-first", "Bash"),
        journalEvent(3, "PostToolUse", "command completed", "Bash"),
        journalEvent(4, "Notification", "Waiting for tool results")};
    view.setActivity({}, events); QTest::qWait(20);
    auto *browser = view.browser();
    QVERIFY(browser->toPlainText().contains("1 tool call, 3 events"));
    QVERIFY(!browser->toPlainText().contains("hidden-command-first"));
    QVERIFY(links(browser).contains("hgs-activity:group-2"));
    activate(browser, "hgs-activity:group-2");
    QVERIFY(browser->toPlainText().contains("hidden-command-first"));
    events.append(journalEvent(5, "Stop", "Review **finished**.")); view.setActivity({}, events);
    QVERIFY(browser->toPlainText().contains("hidden-command-first"));
    activate(browser, "hgs-activity:group-2");
    QVERIFY(!browser->toPlainText().contains("hidden-command-first"));
    events.append(journalEvent(6, "PermissionRequest", "Approve protected operation")); view.setActivity({}, events);
    QVERIFY(browser->toPlainText().contains("Approve protected operation"));
    const auto expandedAttentionLength = browser->toPlainText().size();
    activate(browser, "hgs-activity:group-6");
    QVERIFY(browser->toPlainText().size() < expandedAttentionLength);
    view.setSessionKey("mac/new"); view.setActivity({}, events);
    QVERIFY(!browser->toPlainText().contains("hidden-command-first"));
}

void TestActivityView::rejectsMarkupResourcesAndUnsafeLinks()
{
    ActivityView view; view.resize(540, 320); view.show();
    const QString text = "**Good prose**\n\n<script>bad()</script><img src='file:///etc/passwd'>\n\n"
        "![tracking](https://example.invalid/tracker.png)\n\n[bad](javascript:alert(1)) [file](file:///etc/passwd) "
        "[forged](hgs-activity:group-2) [safe](https://example.com/docs)\n\n```sh\necho '<script>'\n```";
    view.setActivity({}, {journalEvent(1, "Stop", text), journalEvent(2, "PreToolUse", "literal <b>tool</b>", "<tool>")});
    auto *browser = view.browser();
    const auto allLinks = links(browser);
    QVERIFY(allLinks.contains("https://example.com/docs"));
    for (const auto &link : allLinks) QVERIFY(link.startsWith("https://") || link.startsWith("hgs-file:") || link == "hgs-activity:group-2");
    QVERIFY(browser->toPlainText().contains("[Image attachment]"));
    for (auto block = browser->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(!it.fragment().charFormat().isImageFormat() || it.fragment().charFormat().toImageFormat().name().startsWith("hgs-md:"));
    QVERIFY(browser->document()->resource(QTextDocument::ImageResource, QUrl("file:///etc/passwd")).value<QImage>().isNull());
    QSignalSpy opened(&view, &ActivityView::externalLinkActivated);
    activate(browser, "javascript:alert(1)"); activate(browser, "file:///etc/passwd");
    activate(browser, "https://user:password@example.com/private");
    QCOMPARE(opened.size(), 0);
    activate(browser, "hgs-activity:group-2");
    QVERIFY(browser->toPlainText().contains("literal <b>tool</b>"));
}

void TestActivityView::preservesLiteralUserMessages_data()
{
    QTest::addColumn<QString>("source");
    QTest::addColumn<bool>("dark");
    for (const auto &source : QStringList{"UserPromptSubmit", "UserPromptQueued", "TurnStarted",
             "QuestionAnswered", "sending", "sent", "error", "snapshot", "fallback", "search"}) {
        for (bool dark : {false, true})
            QTest::newRow(qPrintable(source + (dark ? "-dark" : "-light"))) << source << dark;
    }
}

void TestActivityView::preservesLiteralUserMessages()
{
    QFETCH(QString, source);
    QFETCH(bool, dark);
    const QString text = "> Вычеты из наших выплат за ошибки игр сейчас без потолка\n"
        "> нужна расшифровка\n\nраскрой что ты имеешь в виду?\n"
        "  >> Вложенная псевдо-цитата\n1. исходный номер\n# не заголовок\n"
        "**буквальные звёздочки** и `код`\n"
        "<b>буквальный HTML</b> & <img src='file:///etc/passwd'>\n"
        "[ссылка](https://example.com)  два пробела";
    ActivityView view; view.resize(420, 800); view.setTheme(dark); view.show();
    view.setSessionKey("arch/quoted-request");
    if (source == "snapshot") view.setActivity({{"prompt", text}}, {});
    else if (source == "fallback") view.setActivity({}, {}, text);
    else if (source == "search") view.showSearchResult(journalEvent(1, "UserPromptSubmit", text), "расшифровка");
    else if (QStringList{"sending", "sent", "error"}.contains(source)) {
        view.setActivity({}, {});
        view.setLocalMessages({QJsonObject{{"id", "quoted"}, {"text", text}, {"status", source}}});
    } else view.setActivity({}, {journalEvent(1, source, text)});
    auto *browser = view.browser();
    QVERIFY2(browser->toPlainText().contains(text), qPrintable(browser->toPlainText()));
    // Selection/copy must preserve the actual prompt as well as its appearance.
    QTextCursor cursor(browser->document());
    cursor.setPosition(browser->toPlainText().indexOf(text));
    cursor.setPosition(cursor.position() + text.size(), QTextCursor::KeepAnchor);
    auto copied = cursor.selectedText();
    copied.replace(QChar::ParagraphSeparator, '\n').replace(QChar::LineSeparator, '\n');
    QCOMPARE(copied, text);
    QVERIFY(!links(browser).contains("https://example.com"));
    for (auto block = browser->document()->begin(); block.isValid(); block = block.next())
        for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(!it.fragment().charFormat().isImageFormat());
    QTest::qWait(10);
    QCOMPARE(browser->horizontalScrollBar()->maximum(), 0);
    const auto directory = qEnvironmentVariable("HGS_PREVIEW_DIR");
    if (!directory.isEmpty() && source == "UserPromptSubmit") {
        QDir().mkpath(directory);
        QVERIFY(view.grab().save(directory + (dark ? "/literal-user-dark.png" : "/literal-user-light.png")));
    }
    view.setActivity({}, {journalEvent(2, "Stop", "**Ответ агента** с `кодом`")});
    if (source == "search") view.clearSearchResult();
    QVERIFY(view.plainText().contains("Ответ агента с кодом"));
    QVERIFY(!browser->toPlainText().contains("**Ответ агента**"));
}

void TestActivityView::preservesSessionFileReferences()
{
    ActivityView view;
    const QString text = "[Package](/home/user/packages/setup.zip)\n\n"
        "[Instructions](files/README.txt) [Registry](<docs/client map.md:12>)\n\n"
        "[**Formatted** label](/tmp/result.txt#L4) [file URI](file:///tmp/report.txt)\n\n"
        "[forged](hgs-file:fake) [network](file://other-host/etc/passwd) [command](command:run)";
    view.setActivity({}, {journalEvent(1, "Stop", text)});
    auto *browser = view.browser();
    const auto rendered = browser->toPlainText();
    QVERIFY(rendered.contains("Package (/home/user/packages/setup.zip)"));
    QVERIFY(rendered.contains("Instructions (files/README.txt)"));
    QVERIFY(rendered.contains("Registry (docs/client map.md:12)"));
    QCOMPARE(rendered.count("/tmp/result.txt#L4"), 1);
    QSignalSpy files(&view, &ActivityView::fileReferenceActivated);
    QSignalSpy external(&view, &ActivityView::externalLinkActivated);
    const auto allLinks = links(browser);
    QVERIFY(!allLinks.contains("hgs-file:fake"));
    for (const auto &link : allLinks) if (link.startsWith("hgs-file:")) activate(browser, link);
    QVERIFY(files.size() >= 5); QCOMPARE(external.size(), 0);
    QVERIFY(files.contains(QList<QVariant>{QString("/home/user/packages/setup.zip")}));
    QVERIFY(files.contains(QList<QVariant>{QString("files/README.txt")}));
    const QString previousLink = allLinks.first();
    view.setSessionKey("other/session"); view.setActivity({}, {journalEvent(1, "Stop", "No files")});
    const auto opened = files.size(); activate(browser, previousLink); QCOMPARE(files.size(), opened);
    const auto target = SessionFileReference::parse("docs/client%20map.md:12");
    QCOMPARE(target.path, QString("docs/client map.md")); QCOMPARE(target.location, QString(":12"));
    QCOMPARE(SessionFileReference::resolve(target, "/remote/project"), QString("/remote/project/docs/client map.md"));
    QVERIFY(SessionFileReference::resolve(target, {}).isEmpty());
    QVERIFY(SessionFileReference::resolve(SessionFileReference::parse("~/result.txt"), "/remote/project").isEmpty());
    QVERIFY(!SessionFileReference::parse("//other-host/path").valid());
    QVERIFY(!SessionFileReference::parse("/tmp/file%0Acommand").valid());
    QCOMPARE(SessionFileReference::parse("LICENSE").path, QString("LICENSE"));
    QCOMPARE(SessionFileReference::parse("README.md#installation").location, QString("#installation"));
}

void TestActivityView::expandsMatchingSnapshotWithoutDuplicate()
{
    ActivityView view;
    view.setActivity({{"prompt", "Please implement this feature completely"}, {"last_message", "Finished the change and ran all tests."}},
        {journalEvent(1, "UserPromptSubmit", "Please implement…"), journalEvent(2, "Stop", "Finished the change…")});
    QCOMPARE(view.browser()->toPlainText().count("Please implement"), 1);
    QCOMPARE(view.browser()->toPlainText().count("Finished the change"), 1);
    QVERIFY(view.browser()->toPlainText().contains("ran all tests."));
    view.setActivity({{"last_message", "An older response"}}, {journalEvent(3, "UserPromptSubmit", "New request")});
    QVERIFY(view.browser()->toPlainText().contains("recorded response"));
    QVERIFY(view.browser()->toPlainText().indexOf("An older response") < view.browser()->toPlainText().indexOf("New request"));
    QVERIFY(view.browser()->toPlainText().contains("Outside the available timeline"));
}

void TestActivityView::taskNotificationIsAColoredNoticeNotYou()
{
    ActivityView view; view.setTheme(true);
    auto notice = journalEvent(2, "UserPromptSubmit", "Background command \"Run checks\" completed (exit code 0)");
    notice["origin"] = "task_notification"; notice["agent_id"] = "";
    view.setActivity({}, {journalEvent(1, "UserPromptSubmit", "Run checks in the background"), notice, journalEvent(3, "Stop", "Checks passed")});
    const auto plain = view.browser()->toPlainText();
    QCOMPARE(plain.count("You"), 1);
    QVERIFY(plain.contains("Background task"));
    QVERIFY(plain.contains("Background command \"Run checks\" completed (exit code 0)"));
    QCOMPARE(plain.count("Sent"), 1);
    QVERIFY(plain.indexOf("Background task") < plain.indexOf("Checks passed"));
    // The notice has its own color in both themes.
    QVERIFY(view.browser()->toHtml().contains("#c5a8f5", Qt::CaseInsensitive));
    view.setTheme(false); view.setActivity({}, {notice});
    QVERIFY(view.browser()->toHtml().contains("#6c43b8", Qt::CaseInsensitive));
}

void TestActivityView::kimiTurnStartedKeepsRequestBeforeTools()
{
    ActivityView view;
    const QString request = "Investigate the dashboard error";
    QJsonArray events{journalEvent(1, "SessionStart", {}), journalEvent(2, "UserPromptSubmit", {}),
        journalEvent(3, "TurnStarted", request), journalEvent(4, "PreToolUse", "Inspect the logs", "Bash"),
        journalEvent(5, "PostToolUse", "Log inspection finished", "Bash"), journalEvent(6, "Stop", "Found the cause")};
    view.setActivity({{"prompt", request}, {"last_message", "Found the cause"}}, events);
    auto plain = view.browser()->toPlainText();
    QCOMPARE(plain.count(request), 1);
    QVERIFY(!plain.contains("UserPromptSubmit"));
    QVERIFY(!plain.contains("RECORDED CONTEXT"));
    QVERIFY(plain.indexOf(request) < plain.indexOf("1 tool call"));
    QVERIFY(plain.indexOf("1 tool call") < plain.indexOf("Found the cause"));
    // Providers that populate both hooks must not duplicate the request.
    events[1] = journalEvent(2, "UserPromptSubmit", request);
    view.setActivity({{"prompt", request}}, events);
    QCOMPARE(view.browser()->toPlainText().count(request), 1);
    // A genuinely repeated request in a later turn remains visible.
    events.append(journalEvent(7, "TurnStarted", request));
    view.setActivity({{"prompt", request}}, events);
    QCOMPARE(view.browser()->toPlainText().count(request), 2);
    auto child = journalEvent(8, "TurnStarted", "Child task prompt"); child["agent_id"] = "worker";
    events.append(child); view.setActivity({}, events);
    QVERIFY(links(view.browser()).contains("hgs-activity:group-8"));
}

void TestActivityView::confirmedQuestionAnswerStaysInTimeline()
{
    ActivityView view;
    auto confirmed = journalEvent(3, "QuestionAnswered", "Which scope? → Documents");
    confirmed["seq"] = 5; // receipt is journaled after a fast subsequent agent response
    view.setActivity({{"prompt", "Ask about scope"}}, {
        journalEvent(1, "TurnStarted", "Ask about scope"),
        journalEvent(2, "PreToolUse", "", "AskUserQuestion"),
        journalEvent(4, "Stop", "I will inspect the documents."), confirmed});
    const auto plain = view.browser()->toPlainText();
    QVERIFY(plain.contains("You (answer)"));
    QVERIFY(plain.indexOf("AskUserQuestion") < plain.indexOf("Which scope?"));
    QVERIFY(plain.indexOf("Which scope?") < plain.indexOf("I will inspect"));
    QCOMPARE(plain.count("Which scope?"), 1);
}

void TestActivityView::nativeQuestionReplies_data()
{
    QTest::addColumn<QString>("source");
    QTest::addColumn<QString>("format");
    QTest::addColumn<bool>("dark");
    for (const auto &source : QStringList{"journal", "snapshot", "local", "search"})
        for (const auto &format : QStringList{"array", "object", "ide"})
            for (bool dark : {false, true})
                QTest::newRow(qPrintable(source + '-' + format + (dark ? "-dark" : "-light"))) << source << format << dark;
}

void TestActivityView::nativeQuestionReplies()
{
    QFETCH(QString, source);
    QFETCH(QString, format);
    QFETCH(bool, dark);
    const QString question = "Which review should run before the release?";
    const QString answer = "Review API compatibility and the migration notes.";
    const QJsonObject reply{{"questionItemId", "[\"request_user_input_async\",\"call_scope\",0]"},
        {"question", question}, {"answer", answer}};
    const QJsonObject second{{"questionItemId", "[\"request_user_input_async\",\"call_scope\",1]"},
        {"question", "Include macOS in the checks?"}, {"answer", "Yes, include the desktop client."}};
    const auto payload = format == "object" ? QJsonDocument(reply) : QJsonDocument(QJsonArray{reply, second});
    QString text = "<send_user_message_question_reply>\n" + QString::fromUtf8(payload.toJson(QJsonDocument::Compact))
        + "\n</send_user_message_question_reply>";
    if (format == "ide") text.prepend("# Context from my IDE setup:\nOpen files: src/example.rs\n## My request for Codex:\n");
    ActivityView view; view.resize(560, 500); view.setTheme(dark); view.setSessionKey("codex/sample/review"); view.show();
    if (source == "snapshot") view.setActivity({{"prompt", text}}, {});
    else if (source == "local") view.setTimeline({}, {}, {QJsonObject{{"id", "reply"}, {"text", text}, {"status", "sent"}}});
    else if (source == "search") view.showSearchResult(journalEvent(1, "UserPromptSubmit", text), "compatibility");
    else view.setActivity({{"prompt", text}}, {journalEvent(1, "UserPromptSubmit", text)});
    auto *browser = view.browser();
    const auto plain = browser->toPlainText();
    QVERIFY(plain.contains(source == "snapshot" ? "You (recorded answer)" : "You (answer)"));
    QCOMPARE(plain.count(question), 1); QCOMPARE(plain.count(answer), 1);
    QVERIFY(plain.indexOf(question) < plain.indexOf("Your answer"));
    QVERIFY(plain.indexOf("Your answer") < plain.indexOf(answer));
    if (format != "object") {
        QCOMPARE(plain.count("Include macOS in the checks?"), 1);
        QCOMPARE(plain.count("Yes, include the desktop client."), 1);
        QVERIFY(plain.indexOf(answer) < plain.indexOf("Include macOS"));
    }
    QVERIFY(!plain.contains("send_user_message_question_reply"));
    QVERIFY(!plain.contains("questionItemId")); QVERIFY(!plain.contains("call_scope"));
    QVERIFY(!plain.contains("Context from my IDE"));
    QVERIFY(links(browser).isEmpty());
    const auto revision = browser->document()->revision();
    if (source == "journal") {
        view.setActivity({{"prompt", text}}, {journalEvent(1, "UserPromptSubmit", text)});
        QCOMPARE(browser->document()->revision(), revision);
    }
    const auto preview = qEnvironmentVariable("HGS_QUESTION_REPLY_PREVIEW");
    if (!preview.isEmpty() && source == "journal" && format == "array") {
        QDir().mkpath(preview); QTest::qWait(30);
        QVERIFY(view.grab().save(preview + (dark ? "/question-reply-dark.png" : "/question-reply-light.png")));
    }
}

void TestActivityView::unknownQuestionReplyMessages_data()
{
    QTest::addColumn<QString>("text");
    const QString start = "<send_user_message_question_reply>\n";
    const QString end = "\n</send_user_message_question_reply>";
    const QString valid = "{\"questionItemId\":\"reply-1\",\"question\":\"Which scope?\",\"answer\":\"Full review\"}";
    QTest::newRow("ordinary-prose") << "Review **these** changes.\nKeep the original text.";
    QTest::newRow("invalid-json") << start + "not json" + end;
    QTest::newRow("truncated") << start + valid;
    QTest::newRow("empty-array") << start + "[]" + end;
    QTest::newRow("missing-id") << start + "{\"question\":\"Which scope?\",\"answer\":\"Full review\"}" + end;
    QTest::newRow("non-text-answer") << start + "{\"questionItemId\":\"reply-1\",\"question\":\"Which scope?\",\"answer\":[\"Full review\"]}" + end;
    QTest::newRow("mixed-validity") << start + '[' + valid + ",false]" + end;
    QTest::newRow("surrounding-prose") << "An example reply:\n" + start + valid + end;
    QTest::newRow("code-example") << "```text\n" + start + valid + end + "\n```";
}

void TestActivityView::unknownQuestionReplyMessages()
{
    QFETCH(QString, text);
    ActivityView view;
    view.setActivity({}, {journalEvent(1, "UserPromptSubmit", text)});
    const auto plain = view.browser()->toPlainText();
    QVERIFY2(plain.contains(text), qPrintable(plain));
    QVERIFY(!plain.contains("You (answer)"));
    QVERIFY(!plain.contains("Your answer"));
}

void TestActivityView::nativeQuestionRepliesPreserveLiteralText()
{
    const QString question = "<img src='file:///private/missing.png'>\nChoose **scope** & priority";
    const QString answer = "> Keep <b>literal markup</b> & `code`\n1. Keep numbering\n[Docs](https://example.com)";
    const auto payload = QJsonDocument(QJsonObject{{"questionItemId", "reply-literal"}, {"question", question}, {"answer", answer}});
    const QString text = "<send_user_message_question_reply>\n" + QString::fromUtf8(payload.toJson(QJsonDocument::Compact))
        + "\n</send_user_message_question_reply>";
    for (bool dark : {false, true}) {
        ActivityView view; view.setTheme(dark);
        view.setActivity({}, {journalEvent(1, "UserPromptSubmit", text)});
        const auto plain = view.browser()->toPlainText();
        QVERIFY(plain.contains(question)); QVERIFY(plain.contains(answer));
        QVERIFY(links(view.browser()).isEmpty());
        for (auto block = view.browser()->document()->begin(); block.isValid(); block = block.next())
            for (auto it = block.begin(); !it.atEnd(); ++it) QVERIFY(!it.fragment().charFormat().isImageFormat());
    }
}

void TestActivityView::unmatchedContextPrecedesTimeline()
{
    ActivityView view; view.resize(560, 360); view.show(); view.setSessionKey("arch/one");
    view.setActivity({{"prompt", "Request missing from journal"}, {"last_message", "Response outside the journal"}}, history(30));
    QTest::qWait(20); // let QTextDocument finish its deferred layout
    auto *browser = view.browser(); auto *bar = browser->verticalScrollBar();
    QTRY_VERIFY(bar->maximum() > 0); QTRY_COMPARE(bar->value(), bar->maximum());
    const auto latest = browser->document()->find("Message number 30");
    QVERIFY(!latest.isNull());
    const QRect answerRect = browser->cursorRect(latest);
    QVERIFY(answerRect.top() >= 0); QVERIFY(answerRect.bottom() <= browser->viewport()->height());
    const auto plain = browser->toPlainText();
    QVERIFY(plain.indexOf("Request missing from journal") < plain.indexOf("Message number 1"));
    QVERIFY(plain.indexOf("Response outside the journal") < plain.indexOf("Message number 1"));
    QVERIFY(plain.indexOf("Message number 1") < plain.indexOf("Message number 30"));
}

void TestActivityView::failedMessageActionsFollowCardIdentity()
{
    ActivityView view;view.resize(600,350);view.show();view.setTheme(true);view.setSessionKey("session-one");
    QSignalSpy actions(&view,&ActivityView::messageActionRequested);
    const QJsonArray events{journalEvent(1,"Stop","Earlier response"),journalEvent(4,"Stop","Later response")};
    view.setActivity({},events);
    QJsonObject message{{"id","failed-one"},{"text","Original failed request"},{"status","error"},{"error","Terminal was not ready"},{"submitted_at",1791018002.0}};
    view.setLocalMessages({message});auto text=view.browser()->toPlainText();
    QVERIFY(text.indexOf("Original failed request")<text.indexOf("Later response"));QVERIFY(text.contains("Retry"));QVERIFY(text.contains("Delete"));
    QStringList commands;for(const auto &url:links(view.browser()))if(url.startsWith("hgs-message:"))commands.append(url);
    QCOMPARE(commands.size(),2);activate(view.browser(),commands[0]);QCOMPARE(actions.size(),1);
    QCOMPARE(actions[0][0].toString(),QString("failed-one"));QCOMPARE(actions[0][1].toString(),QString("retry"));
    activate(view.browser(),commands[1]);QCOMPARE(actions[1][1].toString(),QString("delete"));
    const auto preview=qEnvironmentVariable("HGS_FAILED_MESSAGE_PREVIEW");if(!preview.isEmpty()){QTest::qWait(30);QVERIFY(view.grab().save(preview));}
    message["uncertain"]=true;view.setLocalMessages({message});text=view.browser()->toPlainText();
    QVERIFY(text.contains("Delivery not confirmed"));QVERIFY(text.contains("Check Terminal"));QVERIFY(!text.contains("Retry"));
    activate(view.browser(),commands[0]);QCOMPARE(actions.size(),2); // Old Retry is no longer registered.
    view.setActivity({{"native_ui_available",true}},events);QVERIFY(view.browser()->toPlainText().contains("Check native UI"));
    view.setSessionKey("other");activate(view.browser(),commands[1]);QCOMPARE(actions.size(),2);
}

void TestActivityView::localDeliveryCards()
{
    ActivityView view; view.setSessionKey("arch/one"); view.setActivity({}, history(2));
    QJsonObject message{{"id", "outgoing-1"}, {"text", "Look at **this image**"}, {"status", "sending"},
        {"attachments", QJsonArray{QJsonObject{{"name", "screenshot.png"}}}}};
    view.setLocalMessages({message});
    QVERIFY(view.browser()->toPlainText().contains("Sending…"));
    QVERIFY(view.browser()->toPlainText().contains("Attachment: screenshot.png"));
    message["status"] = "sent"; view.setLocalMessages({message});
    QVERIFY(view.browser()->toPlainText().contains("Submitted to terminal"));
    message["status"] = "error"; message["error"] = "Connection interrupted"; view.setLocalMessages({message});
    QVERIFY(view.browser()->toPlainText().contains("Not sent"));
    QVERIFY(view.browser()->toPlainText().contains("Connection interrupted"));
    view.setSessionKey("mac/two"); view.setActivity({}, {});
    QVERIFY(!view.browser()->toPlainText().contains("screenshot.png"));
}

void TestActivityView::preview()
{
    const auto directory = qEnvironmentVariable("HGS_PREVIEW_DIR");
    if (directory.isEmpty()) QSKIP("Set HGS_PREVIEW_DIR to render the activity preview");
    QDir().mkpath(directory);
    for (bool dark : {true, false}) {
        ActivityView view; view.resize(630, 750); view.setTheme(dark); view.show();
        view.setActivity({}, {
            journalEvent(1, "UserPromptSubmit", "Make the activity easier to read.\n\nKeep **messages** separate from tool output and show the latest event by default."),
            journalEvent(2, "PreToolUse", "rg -n renderDetails tray/src/SessionsWindow.cpp", "Bash"),
            journalEvent(3, "PostToolUse", "Found the activity rendering path", "Bash"),
            journalEvent(4, "PreToolUse", "Add ActivityView and preserve the reading position", "apply_patch"),
            journalEvent(5, "PostToolUse", "Updated ActivityView.cpp", "apply_patch"),
            QJsonObject{{"seq", 6}, {"at", 1791018006}, {"type", "UserPromptSubmit"}, {"agent_id", ""}, {"origin", "task_notification"},
                {"detail", "Background command \"Run focused checks\" completed (exit code 0)"}},
            journalEvent(7, "Stop", "## Summary\n\nFixed launching sessions with a space in the name:\n\n"
                "- `src/cli.rs` — `--new` now follows `rename`\n- `tests/test_hgs.sh` — new checks\n  - nested item with **bold** text\n"
                "- see [docs](https://example.com) and [client registry](docs/client.md:24)\n\n1. First step\n2. Second step\n\n"
                "| File | Lines | Status |\n|------|------:|--------|\n| `src/cli.rs` | 7 | changed |\n| `tests/test_hgs.sh` | 18 | added |\n| `README.md` | 0 | unchanged |\n\n"
                "> Remote machines must update `hgs`.\n\n```sh\nctest --test-dir tray/build -R activityview\n```\n\n---\n\nAll focused checks passed.")});
        QTest::qWait(40);
        QVERIFY(view.grab().save(directory + (dark ? "/activity-dark.png" : "/activity-light.png")));
    }
}

void TestActivityView::caughtUpActivityAcknowledgesEarlierReplyWithoutIntermediatePaint()
{
    ActivityView view;view.resize(560,250);view.show();view.setSessionKey("polling");
    QJsonObject details{{"conversation_id","same"},{"reply_id","10:100"},{"activity","busy"}};
    QJsonArray events{QJsonObject{{"seq",10},{"type","Stop"},{"at",100},{"detail","Completed the previous step."}}};
    for(int i=11;i<120;++i)events.append(QJsonObject{{"seq",i},{"at",100+i},{"type","AgentMessage"},{"detail",QString("Progress update %1").arg(i)}});
    view.setActivity(details,events);QTest::qWait(20);view.jumpToLatest();QTest::qWait(20);
    QVERIFY(view.replyVisible("10:100"));
    auto *browser=view.browser();browser->verticalScrollBar()->setValue(0);QVERIFY(!view.replyVisible("10:100"));
    bool changed=false,paintedDuringUpdate=false;
    connect(browser->document(),&QTextDocument::contentsChanged,&view,[&]{changed=true;paintedDuringUpdate|=browser->updatesEnabled();});
    events.append(QJsonObject{{"seq",120},{"at",220},{"type","AgentMessage"},{"detail","Another update"}});
    view.setActivity(details,events);QVERIFY(changed);QVERIFY(!paintedDuringUpdate);
    QTRY_VERIFY(browser->updatesEnabled());QCOMPARE(browser->verticalScrollBar()->value(),0);
    const auto revision=browser->document()->revision();view.setActivity(details,events);QCOMPARE(browser->document()->revision(),revision);
}

QTEST_MAIN(TestActivityView)
#include "test_activityview.moc"
