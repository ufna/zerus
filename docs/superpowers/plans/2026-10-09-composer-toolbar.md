# Composer Toolbar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace everything between the Activity view and the message field (cache notice, status footer, recovery panel, usage-limit banner, 78 px attachment strip) with one fixed-height toolbar row of compact chips and popovers above the field.

**Architecture:** A new `ComposerToolbar.{h,cpp}` provides `ToolbarChip` (a chip with tone, full/short label and optional popover), `ChipPopover` (anchored `Qt::Popup` panel) and `ComposerToolbar` (a 34 px row whose slot table fixes zone, order and priority). `MessageComposer` owns the toolbar and its attachments chip; `SessionsWindow` adds its own chips (Mark as read, compaction, usage limit, recovery, cache, context) through `toolbar()->add(Slot, widget)`. Existing state functions (`CacheStatus::expired/warning`, `RecoveryUi::status`, `AccountUsage::exhausted`) stay the single source of truth.

**Tech Stack:** C++17, Qt 6.11 Widgets, Qt Test (offscreen), CMake (Unix Makefiles).

**Spec:** `docs/superpowers/specs/2026-10-09-composer-toolbar-design.md` — read it before Task 1. Tracking issue: `zerus-sdr`.

## Global Constraints

- Toolbar row height is always `34` px (`ComposerToolbar::RowHeight`); chips are `24` px high, radius `6` px, font `11px`. The row never wraps and never changes height.
- The toolbar is not scaled by the content scale (75–200 %); only the message field is.
- Tone colours, dark / light — Danger text `#ff9ca8` / `#b52d48`, Warning `#efbd78` / `#91621a`, Success `#72cdb2` / `#237a62`, Quiet `#a1adbb` / `#647386`, Neutral `#e8edf4` / `#1a2733`; accent (flash, keyboard focus) `#8bdfc0` / `#167357`.
- Compaction priority, lowest first: Mark as read (10), attachments (20), cache (30), recovery (40), usage limit (50). Only Mark as read may be hidden. Notices and the context counter are never hidden.
- No change to the `hgs` CLI, `HGS_*` variables, state/config paths, draft files (`ComposerDraftStore`) or `MessageAttachment`.
- Code, comments, UI strings and docs are English. Match the surrounding dense style of `SessionsWindow.cpp`/`MessageComposer.cpp`.
- Never restart the Zerus GUI, never touch tmux servers or live agents. `tray/build/hgs-tray` may be the running GUI: build **only test targets** in `tray/build` (`--target test_…`), never `hgs-tray` or `all` there. The full suite runs in the separate `.ci-build/gui` directory (Task 8).
- Other sessions edit this worktree concurrently: run `git status --short <file>` before editing a file, use the Edit tool (never a script that truncates a file before reading it), and commit only own paths with `git commit --only -- <paths>`. Commit directly on `main`, without any Claude attribution line, and do not push.

## Review Focus

- A popover left open while the user switches session must close, never show the previous session's attachments or recovery state — tests in Task 2 (attachments) and Task 6 (usage limit), Task 7 (recovery).
- Removing an attachment from an open popover must refresh the list in place and close the popover with the last one — test in Task 2.
- Sending while the attachments popover is open must close it and hide the chip; a failed delivery brings the chip back — test in Task 2.
- A theme switch must recolour chips added before and after it, including chips added by `SessionsWindow` and their popovers — test in Task 1.
- Recovery becoming inactive while its popover holds keyboard focus must close the popover and return focus to the message field — test in Task 7.

## Build and test commands

```bash
# Build one test target (reconfigures automatically after CMakeLists changes)
cmake --build tray/build --target test_composertoolbar -j16
# Run a whole suite / one function
QT_QPA_PLATFORM=offscreen tray/build/tests/test_composertoolbar
QT_QPA_PLATFORM=offscreen tray/build/tests/test_sessionswindow coldCacheClearKeepsDraftAndPinsIdentity
# Run the sharded window suite
ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure
```

---

### Task 1: `ComposerToolbar` component

**Files:**
- Create: `tray/src/ComposerToolbar.h`, `tray/src/ComposerToolbar.cpp`
- Create: `tray/tests/test_composertoolbar.cpp`
- Modify: `tray/CMakeLists.txt:54` (app sources), `tray/tests/CMakeLists.txt` (new target after the `test_messagecomposer` block)

**Interfaces:**
- Consumes: `workspaceIcon(const QString &, const QColor &)` (`WorkspaceIcons.h`), `setWorkspaceStyle(QWidget *, const QString &)` (`WorkspaceStyle.h`).
- Produces (used by every later task):
  - `enum class ChipTone { Quiet, Neutral, Success, Warning, Danger };`
  - `class ChipPopover : QFrame` — `setContent(QWidget *)`, `content()`, `setTheme(bool)`, `showFor(QWidget *anchor)`, `reposition()`.
  - `class ToolbarChip : QPushButton` — `setLabels(full, shortText = {})`, `fullLabel()`, `shortLabel()`, `setDetail(QString)`, `setTone(ChipTone)`, `tone()`, `setIconName(QString)`, `setTheme(bool)`, `setActive(bool)`, `isActive()`, `setCompact(bool)`, `isCompact()`, `setPopoverContent(QWidget *)`, `popover()`, `openPopover()`, `closePopover()`, `flash()` (sets property `"flashing"` for one second); signal `fitChanged()`.
  - `class ComposerToolbar : QWidget` — `enum class Slot { Attachments, MarkRead, Compaction, CompactionCancel, UsageLimit, Recovery, Cache, Context }`, `static constexpr int RowHeight = 34`, `add(Slot, QWidget *)`, `setTheme(bool)`, `closePopovers()`, `fit()`.

- [ ] **Step 1: Check the files are free**

Run: `git status --short tray/CMakeLists.txt tray/tests/CMakeLists.txt tray/src/ComposerToolbar.h tray/src/ComposerToolbar.cpp tray/tests/test_composertoolbar.cpp`
Expected: no output. If another session has uncommitted hunks in a CMake file, edit around them and stage only your own hunks later.

- [ ] **Step 2: Write the failing test**

Create `tray/tests/test_composertoolbar.cpp`:

```cpp
#include "ComposerToolbar.h"

#include <QApplication>
#include <QLineEdit>
#include <QPushButton>
#include <QSignalSpy>
#include <QTest>
#include <QVBoxLayout>

namespace {
ToolbarChip *makeChip(const char *name, const QString &full, const QString &shortText, const QString &icon, ChipTone tone)
{
    auto *chip = new ToolbarChip; chip->setObjectName(name); chip->setLabels(full, shortText);
    chip->setIconName(icon); chip->setTone(tone); chip->setActive(true); return chip;
}

struct Row {
    QWidget window;
    QLineEdit *field = new QLineEdit;
    ComposerToolbar *toolbar = new ComposerToolbar;
    ToolbarChip *attachments = makeChip("attachments", "3 attached", "3", "attachment", ChipTone::Neutral);
    ToolbarChip *read = makeChip("read", "Mark as read", {}, "read-all", ChipTone::Quiet);
    ToolbarChip *recovery = makeChip("recovery", "Retry in 42 s", "42 s", "refresh", ChipTone::Warning);
    ToolbarChip *cache = makeChip("cache", "Cold cache", "Cold", "context-warning", ChipTone::Danger);
    QPushButton *context = new QPushButton("54% (139k/258k)");
    Row() {
        auto *layout = new QVBoxLayout(&window); layout->setContentsMargins(0, 0, 0, 0); layout->setSpacing(0);
        layout->addWidget(toolbar); layout->addWidget(field);
        context->setObjectName("context"); context->setFixedHeight(24); context->setFocusPolicy(Qt::TabFocus);
        // Added out of order on purpose: slots, not calls, decide the visual order.
        toolbar->add(ComposerToolbar::Slot::Context, context);
        toolbar->add(ComposerToolbar::Slot::Cache, cache);
        toolbar->add(ComposerToolbar::Slot::MarkRead, read);
        toolbar->add(ComposerToolbar::Slot::Recovery, recovery);
        toolbar->add(ComposerToolbar::Slot::Attachments, attachments);
    }
    void show(int width) { window.resize(width, 90); window.show(); }
};

QRect bounds(QWidget *widget, QWidget *root) { return QRect(widget->mapTo(root, QPoint()), widget->size()); }

// Shortening follows priority, notices never disappear and nothing overlaps.
void verifyFits(Row &row)
{
    QCOMPARE(row.toolbar->height(), ComposerToolbar::RowHeight);
    const QList<ToolbarChip *> lowestFirst{row.read, row.attachments, row.cache, row.recovery};
    bool higherShortened = false;
    for (qsizetype i = lowestFirst.size() - 1; i >= 0; --i) {
        if (higherShortened) QVERIFY2(lowestFirst[i]->isCompact() || lowestFirst[i]->isHidden(), qPrintable(lowestFirst[i]->objectName()));
        higherShortened = higherShortened || lowestFirst[i]->isCompact();
    }
    for (QWidget *always : QList<QWidget *>{row.attachments, row.recovery, row.cache, row.context})
        QVERIFY2(always->isVisible(), qPrintable(always->objectName()));
    if (row.read->isHidden()) for (auto *chip : {row.attachments, row.cache, row.recovery}) QVERIFY(chip->isCompact());
    int right = -1;
    for (QWidget *item : QList<QWidget *>{row.attachments, row.read, row.recovery, row.cache, row.context}) {
        if (item->isHidden()) continue;
        const QRect rect = bounds(item, row.toolbar);
        QVERIFY2(rect.left() > right, qPrintable(item->objectName()));
        QVERIFY2(rect.right() < row.toolbar->width(), qPrintable(item->objectName()));
        QVERIFY(qAbs(rect.center().y() - row.toolbar->height() / 2) <= 1);
        right = rect.right();
    }
}
}

class TestComposerToolbar : public QObject {
    Q_OBJECT
private slots:
    void slotsFixOrderAndRowHeight();
    void narrowRowsShortenInPriorityOrder_data();
    void narrowRowsShortenInPriorityOrder();
    void compactChipsKeepFullAccessibleNames();
    void popoverOpensFromMouseAndKeyboardAndRestoresFocus();
    void themeReachesChipsAddedBeforeAndAfter();
    void flashHighlightsBriefly();
};

void TestComposerToolbar::slotsFixOrderAndRowHeight()
{
    Row row; row.show(900); QVERIFY(QTest::qWaitForWindowExposed(&row.window));
    QTRY_VERIFY(row.context->isVisible());
    verifyFits(row); if (QTest::currentTestFailed()) return;
    const int fieldTop = row.field->y();
    for (auto *chip : {row.attachments, row.read, row.recovery, row.cache}) chip->setActive(false);
    row.context->hide(); QCoreApplication::processEvents();
    QCOMPARE(row.toolbar->height(), ComposerToolbar::RowHeight); QCOMPARE(row.field->y(), fieldTop);
    row.cache->setActive(true); QTRY_VERIFY(row.cache->isVisible()); QCOMPARE(row.field->y(), fieldTop);
}

void TestComposerToolbar::narrowRowsShortenInPriorityOrder_data()
{
    QTest::addColumn<int>("width");
    for (int width : {900, 640, 420, 320}) QTest::newRow(qPrintable(QString::number(width))) << width;
}

void TestComposerToolbar::narrowRowsShortenInPriorityOrder()
{
    QFETCH(int, width);
    Row row; row.show(900); QVERIFY(QTest::qWaitForWindowExposed(&row.window));
    QTRY_VERIFY(row.context->isVisible());
    for (auto *chip : {row.attachments, row.read, row.recovery, row.cache}) QVERIFY(!chip->isCompact());
    row.window.resize(width, 90); QTRY_COMPARE(row.toolbar->width(), width);
    QCoreApplication::processEvents();
    verifyFits(row); if (QTest::currentTestFailed()) return;
    if (width <= 320) QVERIFY(row.read->isCompact() || row.read->isHidden());
    // Widening restores every full label and Mark as read.
    row.window.resize(900, 90);
    QTRY_VERIFY(!row.recovery->isCompact() && !row.read->isCompact() && row.read->isVisible());
}

void TestComposerToolbar::compactChipsKeepFullAccessibleNames()
{
    Row row; row.recovery->setDetail("Provider overloaded. The next attempt starts in 42 s.");
    row.show(320); QVERIFY(QTest::qWaitForWindowExposed(&row.window));
    QTRY_VERIFY(row.attachments->isCompact());
    for (auto *chip : {row.attachments, row.recovery, row.cache}) QCOMPARE(chip->accessibleName(), chip->fullLabel());
    QCOMPARE(row.recovery->toolTip(), QString("Provider overloaded. The next attempt starts in 42 s."));
    QCOMPARE(row.recovery->accessibleDescription(), row.recovery->toolTip());
    QCOMPARE(row.cache->toolTip(), QString("Cold cache"));
}

void TestComposerToolbar::popoverOpensFromMouseAndKeyboardAndRestoresFocus()
{
    Row row;
    auto *content = new QWidget; auto *action = new QPushButton("Clear context", content); action->setObjectName("popoverAction");
    (new QVBoxLayout(content))->addWidget(action);
    row.cache->setPopoverContent(content);
    row.show(900); row.window.activateWindow(); QVERIFY(QTest::qWaitForWindowActive(&row.window));
    row.field->setFocus(); QTRY_VERIFY(row.field->hasFocus());
    const auto escape = [&] {
        QWidget *target = QApplication::focusWidget() ? QApplication::focusWidget() : row.cache->popover();
        QTest::keyClick(target, Qt::Key_Escape);
    };
    // Mouse: opens above the chip and gives focus back to the field.
    QTest::mouseClick(row.cache, Qt::LeftButton);
    QTRY_VERIFY(row.cache->popover()->isVisible());
    QVERIFY(row.cache->popover()->geometry().bottom() < row.cache->mapToGlobal(QPoint(0, 0)).y());
    escape(); QTRY_VERIFY(!row.cache->popover()->isVisible()); QTRY_VERIFY(row.field->hasFocus());
    // Keyboard: Enter and Space open it; Escape returns to the chip.
    for (const auto key : {Qt::Key_Return, Qt::Key_Space}) {
        row.cache->setFocus(Qt::TabFocusReason); QTRY_VERIFY(row.cache->hasFocus());
        QTest::keyClick(row.cache, key); QTRY_VERIFY(row.cache->popover()->isVisible());
        escape(); QTRY_VERIFY(!row.cache->popover()->isVisible()); QTRY_VERIFY(row.cache->hasFocus());
    }
    // A chip without a popover only reports the click, and a click never takes focus.
    row.field->setFocus(); QTRY_VERIFY(row.field->hasFocus());
    QSignalSpy clicked(row.attachments, &QPushButton::clicked);
    QTest::mouseClick(row.attachments, Qt::LeftButton); QCOMPARE(clicked.size(), 1); QVERIFY(row.field->hasFocus());
    // Deactivating a chip closes its popover.
    QTest::mouseClick(row.cache, Qt::LeftButton); QTRY_VERIFY(row.cache->popover()->isVisible());
    row.cache->setActive(false); QVERIFY(!row.cache->popover()->isVisible());
}

void TestComposerToolbar::themeReachesChipsAddedBeforeAndAfter()
{
    Row row; row.toolbar->setTheme(false);
    QVERIFY(row.cache->styleSheet().contains("#b52d48")); QVERIFY(row.recovery->styleSheet().contains("#91621a"));
    auto *late = makeChip("late", "Limit reached", "Limit", "attention", ChipTone::Danger);
    row.toolbar->add(ComposerToolbar::Slot::UsageLimit, late); QVERIFY(late->styleSheet().contains("#b52d48"));
    late->setPopoverContent(new QWidget); QVERIFY(late->popover()->styleSheet().contains("#f8fafb"));
    row.toolbar->setTheme(true);
    QVERIFY(late->styleSheet().contains("#ff9ca8")); QVERIFY(late->popover()->styleSheet().contains("#171d24"));
}

void TestComposerToolbar::flashHighlightsBriefly()
{
    Row row; row.show(900); QVERIFY(QTest::qWaitForWindowExposed(&row.window));
    const auto before = row.attachments->styleSheet();
    row.attachments->flash();
    QVERIFY(row.attachments->property("flashing").toBool()); QVERIFY(row.attachments->styleSheet() != before);
    QTRY_VERIFY_WITH_TIMEOUT(!row.attachments->property("flashing").toBool(), 2000);
    QCOMPARE(row.attachments->styleSheet(), before);
}

QTEST_MAIN(TestComposerToolbar)
#include "test_composertoolbar.moc"
```

Register it in `tray/tests/CMakeLists.txt`, directly after the `test_messagecomposer` block:

```cmake
add_executable(test_composertoolbar test_composertoolbar.cpp ../src/ComposerToolbar.cpp)
target_include_directories(test_composertoolbar PRIVATE ../src)
target_link_libraries(test_composertoolbar PRIVATE Qt6::Test Qt6::Widgets)
add_test(NAME composertoolbar COMMAND test_composertoolbar)
set_tests_properties(composertoolbar PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cmake --build tray/build --target test_composertoolbar -j16`
Expected: build FAILS — `ComposerToolbar.h: No such file or directory` (or missing `ComposerToolbar.cpp`).

- [ ] **Step 4: Write the header**

Create `tray/src/ComposerToolbar.h`:

```cpp
#pragma once

#include <QFrame>
#include <QList>
#include <QPointer>
#include <QPushButton>

class QHBoxLayout;
class QTimer;

enum class ChipTone { Quiet, Neutral, Success, Warning, Danger };

// Popup panel anchored above a chip. Escape closes it; closing returns focus to
// the widget that had it at opening: the chip after keyboard activation, the
// message field after a mouse click (chips never take focus on click).
class ChipPopover : public QFrame {
public:
    explicit ChipPopover(QWidget *parent);
    void setContent(QWidget *content);
    QWidget *content() const { return m_content; }
    void setTheme(bool dark);
    void showFor(QWidget *anchor);
    // Keeps an open popover above its anchor after its content changed size.
    void reposition();
protected:
    void paintEvent(QPaintEvent *event) override;
    void hideEvent(QHideEvent *event) override;
private:
    QPointer<QWidget> m_content, m_anchor, m_returnFocus;
};

// A compact notice or status above the message field. A narrow row shows its
// short label (or only its icon); the full label stays its accessible name.
class ToolbarChip : public QPushButton {
    Q_OBJECT
public:
    explicit ToolbarChip(QWidget *parent = nullptr);
    void setLabels(const QString &full, const QString &shortText = {});
    QString fullLabel() const { return m_full; }
    QString shortLabel() const { return m_short; }
    // The complete notice: tooltip and accessible description.
    void setDetail(const QString &detail);
    void setTone(ChipTone tone);
    ChipTone tone() const { return m_tone; }
    void setIconName(const QString &name);
    void setTheme(bool dark);
    // Owners show and hide a chip only here; the toolbar may additionally hide a
    // hideable chip that does not fit.
    void setActive(bool active);
    bool isActive() const { return m_active; }
    void setFitHidden(bool hidden);
    void setCompact(bool compact);
    bool isCompact() const { return m_compact; }
    QSize labelSizeHint(bool compact) const;
    QSize sizeHint() const override { return labelSizeHint(m_compact); }
    QSize minimumSizeHint() const override;
    void setPopoverContent(QWidget *content);
    ChipPopover *popover() const { return m_popover; }
    void openPopover();
    void closePopover();
    // A one-second accent border confirming that something was added.
    void flash();
signals:
    // Anything the row width depends on changed.
    void fitChanged();
protected:
    void keyPressEvent(QKeyEvent *event) override;
    void paintEvent(QPaintEvent *event) override;
private:
    QString labelText(bool compact) const;
    void render();
    void updateVisibility();
    QString m_full, m_short, m_detail, m_icon, m_renderedIcon;
    ChipTone m_tone = ChipTone::Neutral;
    bool m_dark = true, m_active = false, m_fitHidden = false, m_compact = false, m_flashing = false;
    ChipPopover *m_popover = nullptr;
    QTimer *m_flashTimer;
};

// One fixed-height row above the message field. The slot table fixes each item's
// zone, order and priority; a narrow row shortens low-priority chips first, then
// hides Mark as read, and only then elides the widest chips. It never wraps.
class ComposerToolbar : public QWidget {
    Q_OBJECT
public:
    enum class Slot { Attachments, MarkRead, Compaction, CompactionCancel, UsageLimit, Recovery, Cache, Context };
    static constexpr int RowHeight = 34;
    explicit ComposerToolbar(QWidget *parent = nullptr);
    void add(Slot slot, QWidget *item);
    void setTheme(bool dark);
    void closePopovers();
    // Fits the row now; other changes refit on the next event-loop pass.
    void fit();
protected:
    bool event(QEvent *event) override;
    bool eventFilter(QObject *watched, QEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
private:
    struct Item { Slot slot; QPointer<QWidget> widget; };
    static bool leading(Slot slot);
    static int priority(Slot slot);
    void scheduleFit();
    QList<int> fitInputs() const;
    int requiredWidth() const;
    QList<Item> m_items;
    QHBoxLayout *m_leading, *m_trailing;
    QList<int> m_fitted;
    bool m_dark = true, m_fitting = false, m_scheduled = false;
};
```

- [ ] **Step 5: Write the implementation**

Create `tray/src/ComposerToolbar.cpp`:

```cpp
#include "ComposerToolbar.h"
#include "WorkspaceIcons.h"
#include "WorkspaceStyle.h"

#include <QApplication>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QPainter>
#include <QScreen>
#include <QStyleOption>
#include <QStylePainter>
#include <QTimer>
#include <QVBoxLayout>
#include <algorithm>

namespace {
constexpr int ChipHeight = 24, ChipPadding = 9, IconSize = 14, IconGap = 5, Spacing = 6, NeverShortened = 100;

struct ToneColors { const char *text, *background, *border, *hover; };
ToneColors toneColors(ChipTone tone, bool dark)
{
    switch (tone) {
    case ChipTone::Quiet: return dark ? ToneColors{"#a1adbb", "transparent", "transparent", "#27313b"} : ToneColors{"#647386", "transparent", "transparent", "#e8edf3"};
    case ChipTone::Neutral: return dark ? ToneColors{"#e8edf4", "#222a33", "#3d4855", "#2a3440"} : ToneColors{"#1a2733", "#f1f4f7", "#dce2e8", "#e8edf3"};
    case ChipTone::Success: return dark ? ToneColors{"#72cdb2", "transparent", "transparent", "#27313b"} : ToneColors{"#237a62", "transparent", "transparent", "#e8edf3"};
    case ChipTone::Warning: return dark ? ToneColors{"#efbd78", "#2a2419", "#5a4a2e", "#33291b"} : ToneColors{"#91621a", "#fbf3e4", "#e6cf9f", "#f6e8cc"};
    case ChipTone::Danger: return dark ? ToneColors{"#ff9ca8", "#2a1e24", "#5a3540", "#33222a"} : ToneColors{"#b52d48", "#fdeef0", "#efc2ca", "#f9dde2"};
    }
    return {};
}
QString accent(bool dark) { return dark ? QStringLiteral("#8bdfc0") : QStringLiteral("#167357"); }
}

ChipPopover::ChipPopover(QWidget *parent) : QFrame(parent, Qt::Popup | Qt::FramelessWindowHint)
{
    // Translucent top-level widgets skip Qt's background fill: paintEvent draws it.
    setObjectName("chipPopover"); setAttribute(Qt::WA_TranslucentBackground); setFocusPolicy(Qt::StrongFocus);
}

void ChipPopover::setContent(QWidget *content)
{
    if (!layout()) { auto *box = new QVBoxLayout(this); box->setContentsMargins(0, 0, 0, 0); }
    m_content = content; layout()->addWidget(content); content->show();
}

void ChipPopover::setTheme(bool dark)
{
    setWorkspaceStyle(this, QString("QFrame#chipPopover { background:%1; color:%2; border:1px solid %3; border-radius:9px; }"
        " QFrame#chipPopover QLabel { color:%2; background:transparent; border:0; }")
        .arg(dark ? "#171d24" : "#f8fafb", dark ? "#c9d2dd" : "#1a2733", dark ? "#3d4855" : "#dce2e8"));
}

void ChipPopover::showFor(QWidget *anchor)
{
    m_anchor = anchor; m_returnFocus = QApplication::focusWidget();
    const QRect available = anchor->screen()->availableGeometry();
    const int wanted = qMax(260, m_content ? m_content->sizeHint().width() : 0);
    setFixedWidth(qMin(wanted, qMin(420, available.width() - 24)));
    reposition(); show(); setFocus(Qt::PopupFocusReason);
}

void ChipPopover::reposition()
{
    if (!m_anchor) return;
    adjustSize();
    const QRect available = m_anchor->screen()->availableGeometry();
    const QPoint origin = m_anchor->mapToGlobal(QPoint(0, 0));
    const QWidget *window = m_anchor->window();
    const bool leftHalf = origin.x() + m_anchor->width() / 2 < window->mapToGlobal(window->rect().center()).x();
    int x = leftHalf ? origin.x() : origin.x() + m_anchor->width() - width();
    int y = origin.y() - height() - 6;
    x = qBound(available.left() + 8, x, available.right() - width() - 8);
    y = qBound(available.top() + 8, y, available.bottom() - height() - 8);
    move(x, y);
}

void ChipPopover::paintEvent(QPaintEvent *)
{
    QStyleOption option; option.initFrom(this);
    QPainter painter(this); style()->drawPrimitive(QStyle::PE_Widget, &option, &painter, this);
}

void ChipPopover::hideEvent(QHideEvent *event)
{
    QFrame::hideEvent(event);
    if (m_returnFocus && m_returnFocus->isVisible() && m_returnFocus->isEnabled()) m_returnFocus->setFocus(Qt::PopupFocusReason);
    m_returnFocus = nullptr;
}

ToolbarChip::ToolbarChip(QWidget *parent) : QPushButton(parent), m_flashTimer(new QTimer(this))
{
    setFixedHeight(ChipHeight); setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Fixed);
    setFocusPolicy(Qt::TabFocus); setCursor(Qt::PointingHandCursor); setAutoDefault(false);
    setIconSize(QSize(IconSize, IconSize));
    m_flashTimer->setSingleShot(true); m_flashTimer->setInterval(1000);
    connect(m_flashTimer, &QTimer::timeout, this, [this] { m_flashing = false; setProperty("flashing", false); render(); });
    connect(this, &QPushButton::clicked, this, [this] { if (m_popover && m_popover->content()) openPopover(); });
    render(); updateVisibility();
}

QString ToolbarChip::labelText(bool compact) const
{
    if (!compact) return m_full;
    return m_short.isEmpty() && m_icon.isEmpty() ? m_full : m_short;
}

void ToolbarChip::setLabels(const QString &full, const QString &shortText)
{
    if (full == m_full && shortText == m_short) return;
    m_full = full; m_short = shortText;
    setText(labelText(m_compact)); setAccessibleName(full);
    if (m_detail.isEmpty()) setToolTip(full);
    updateGeometry(); update(); emit fitChanged();
}

void ToolbarChip::setDetail(const QString &detail)
{
    m_detail = detail; setToolTip(detail.isEmpty() ? m_full : detail); setAccessibleDescription(detail);
}

void ToolbarChip::setTone(ChipTone tone) { if (tone == m_tone) return; m_tone = tone; render(); }

void ToolbarChip::setIconName(const QString &name)
{
    if (name == m_icon) return;
    m_icon = name; render(); updateGeometry(); emit fitChanged();
}

void ToolbarChip::setTheme(bool dark) { m_dark = dark; render(); if (m_popover) m_popover->setTheme(dark); }

void ToolbarChip::setActive(bool active)
{
    if (active == m_active) return;
    m_active = active; updateVisibility(); emit fitChanged();
}

void ToolbarChip::setFitHidden(bool hidden) { if (hidden == m_fitHidden) return; m_fitHidden = hidden; updateVisibility(); }

void ToolbarChip::setCompact(bool compact)
{
    if (compact == m_compact) return;
    m_compact = compact; setText(labelText(compact)); updateGeometry(); update();
}

QSize ToolbarChip::labelSizeHint(bool compact) const
{
    ensurePolished();   // the 11 px stylesheet font
    const QString text = labelText(compact);
    int width = 2 * ChipPadding + 2 + fontMetrics().horizontalAdvance(text);
    if (!m_icon.isEmpty()) width += IconSize + (text.isEmpty() ? 0 : IconGap);
    return {width, ChipHeight};
}

QSize ToolbarChip::minimumSizeHint() const { return {2 * ChipPadding + 2 + (m_icon.isEmpty() ? 12 : IconSize), ChipHeight}; }

void ToolbarChip::setPopoverContent(QWidget *content)
{
    if (!m_popover) { m_popover = new ChipPopover(this); m_popover->setTheme(m_dark); }
    m_popover->setContent(content);
}

void ToolbarChip::openPopover() { if (m_popover && m_popover->content() && isVisible()) m_popover->showFor(this); }

void ToolbarChip::closePopover() { if (m_popover && m_popover->isVisible()) m_popover->hide(); }

void ToolbarChip::flash() { m_flashing = true; setProperty("flashing", true); render(); m_flashTimer->start(); }

void ToolbarChip::keyPressEvent(QKeyEvent *event)
{
    if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) { click(); event->accept(); return; }
    QPushButton::keyPressEvent(event);
}

void ToolbarChip::paintEvent(QPaintEvent *)
{
    QStylePainter painter(this); QStyleOptionButton option; initStyleOption(&option);
    const int available = qMax(0, width() - 2 * ChipPadding - 2 - (m_icon.isEmpty() ? 0 : IconSize + IconGap));
    option.text = fontMetrics().elidedText(labelText(m_compact), Qt::ElideRight, available);
    painter.drawControl(QStyle::CE_PushButton, option);
}

void ToolbarChip::render()
{
    // Cache and recovery chips re-render every second: only restyle real changes.
    const auto colors = toneColors(m_tone, m_dark);
    const QString border = m_flashing ? accent(m_dark) : QString(colors.border);
    setWorkspaceStyle(this, QString("QPushButton { font-size:11px; min-height:0; padding:0 %6px; border-radius:6px; text-align:left;"
        " color:%1; background:%2; border:1px solid %3; } QPushButton:hover { background:%4; }"
        " QPushButton:focus[keyboardFocus=\"true\"] { border-color:%5; }")
        .arg(colors.text, colors.background, border, colors.hover, accent(m_dark)).arg(ChipPadding));
    const QString iconKey = m_icon + '|' + colors.text;
    if (iconKey != m_renderedIcon) {
        m_renderedIcon = iconKey; setIcon(m_icon.isEmpty() ? QIcon() : workspaceIcon(m_icon, QColor(colors.text)));
    }
}

void ToolbarChip::updateVisibility()
{
    const bool visible = m_active && !m_fitHidden;
    if (!visible) closePopover();
    setVisible(visible);
}

ComposerToolbar::ComposerToolbar(QWidget *parent) : QWidget(parent)
{
    setObjectName("composerToolbar"); setFixedHeight(RowHeight);
    // Never let the chips widen the composer: the row fits itself instead.
    setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
    auto *row = new QHBoxLayout(this); row->setContentsMargins(0, 0, 0, 0); row->setSpacing(Spacing);
    row->setSizeConstraint(QLayout::SetNoConstraint);
    m_leading = new QHBoxLayout; m_leading->setSpacing(Spacing);
    m_trailing = new QHBoxLayout; m_trailing->setSpacing(Spacing);
    row->addLayout(m_leading); row->addStretch(1); row->addLayout(m_trailing);
}

bool ComposerToolbar::leading(Slot slot) { return slot <= Slot::CompactionCancel; }

int ComposerToolbar::priority(Slot slot)
{
    switch (slot) {
    case Slot::MarkRead: return 10;
    case Slot::Attachments: return 20;
    case Slot::Cache: return 30;
    case Slot::Recovery: return 40;
    case Slot::UsageLimit: return 50;
    default: return NeverShortened;
    }
}

void ComposerToolbar::add(Slot slot, QWidget *item)
{
    int index = 0;
    for (const auto &existing : m_items)
        if (existing.widget && leading(existing.slot) == leading(slot) && existing.slot < slot) ++index;
    (leading(slot) ? m_leading : m_trailing)->insertWidget(index, item, 0, Qt::AlignVCenter);
    m_items.append({slot, item});
    if (auto *chip = qobject_cast<ToolbarChip *>(item)) {
        chip->setTheme(m_dark); connect(chip, &ToolbarChip::fitChanged, this, &ComposerToolbar::scheduleFit);
    } else item->installEventFilter(this);
    scheduleFit();
}

void ComposerToolbar::setTheme(bool dark)
{
    m_dark = dark;
    for (const auto &item : m_items) if (auto *chip = qobject_cast<ToolbarChip *>(item.widget)) chip->setTheme(dark);
}

void ComposerToolbar::closePopovers()
{
    for (const auto &item : m_items) if (auto *chip = qobject_cast<ToolbarChip *>(item.widget)) chip->closePopover();
}

void ComposerToolbar::scheduleFit()
{
    if (m_scheduled || m_fitting) return;
    m_scheduled = true;
    QTimer::singleShot(0, this, [this] { m_scheduled = false; fit(); });
}

bool ComposerToolbar::event(QEvent *event)
{
    // A plain item (context counter) changed its size hint.
    if (event->type() == QEvent::LayoutRequest) scheduleFit();
    return QWidget::event(event);
}

bool ComposerToolbar::eventFilter(QObject *watched, QEvent *event)
{
    if (event->type() == QEvent::Show || event->type() == QEvent::Hide) scheduleFit();
    return QWidget::eventFilter(watched, event);
}

void ComposerToolbar::resizeEvent(QResizeEvent *event) { QWidget::resizeEvent(event); fit(); }

// Everything fit() depends on. Its own compaction changes none of it, so the
// layout requests that compaction causes do not start another pass.
QList<int> ComposerToolbar::fitInputs() const
{
    QList<int> inputs{contentsRect().width()};
    for (const auto &item : m_items) {
        if (!item.widget) continue;
        if (auto *chip = qobject_cast<ToolbarChip *>(item.widget))
            inputs << chip->isActive() << chip->labelSizeHint(false).width() << chip->labelSizeHint(true).width();
        else inputs << item.widget->isVisibleTo(this) << item.widget->sizeHint().width();
    }
    return inputs;
}

int ComposerToolbar::requiredWidth() const
{
    int width = 0, count = 0;
    for (const auto &item : m_items) {
        if (!item.widget || !item.widget->isVisibleTo(this)) continue;
        width += qMin(item.widget->sizeHint().width(), item.widget->maximumWidth()); ++count;
    }
    // The stretch between the zones adds one more gap.
    return width + count * Spacing;
}

void ComposerToolbar::fit()
{
    const auto inputs = fitInputs();
    if (m_fitting || inputs == m_fitted) return;
    m_fitting = true;
    auto order = m_items;
    std::stable_sort(order.begin(), order.end(), [](const Item &a, const Item &b) { return priority(a.slot) < priority(b.slot); });
    QList<ToolbarChip *> chips;   // lowest priority first
    for (const auto &item : order)
        if (auto *chip = qobject_cast<ToolbarChip *>(item.widget); chip && priority(item.slot) < NeverShortened) {
            chip->setCompact(false); chip->setFitHidden(false); chip->setMaximumWidth(QWIDGETSIZE_MAX); chips << chip;
        }
    const int available = contentsRect().width();
    for (auto *chip : chips) {
        if (requiredWidth() <= available) break;
        if (chip->isActive() && chip->labelSizeHint(true).width() < chip->labelSizeHint(false).width()) chip->setCompact(true);
    }
    for (const auto &item : order) {
        if (requiredWidth() <= available) break;
        if (item.slot == Slot::MarkRead) if (auto *chip = qobject_cast<ToolbarChip *>(item.widget)) chip->setFitHidden(true);
    }
    // Last resort: elide the widest chips so the row never wraps.
    int excess = requiredWidth() - available;
    if (excess > 0) {
        QList<ToolbarChip *> widest;
        for (auto *chip : chips) if (chip->isVisibleTo(this)) widest << chip;
        std::sort(widest.begin(), widest.end(), [](ToolbarChip *a, ToolbarChip *b) { return a->sizeHint().width() > b->sizeHint().width(); });
        for (auto *chip : widest) {
            if (excess <= 0) break;
            const int hint = chip->sizeHint().width(), target = qMax(chip->minimumSizeHint().width(), hint - excess);
            chip->setMaximumWidth(target); excess -= hint - target;
        }
    }
    m_fitted = inputs; m_fitting = false;
}
```

Add `src/ComposerToolbar.cpp` to the app in `tray/CMakeLists.txt`, on the line after `src/MessageComposer.cpp`:

```cmake
    src/MessageComposer.cpp
    src/ComposerToolbar.cpp
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cmake --build tray/build --target test_composertoolbar -j16 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_composertoolbar`
Expected: `Totals: 12 passed, 0 failed, 0 skipped` (6 plain functions, 4 data rows of the narrow test, `initTestCase` and `cleanupTestCase`).

- [ ] **Step 7: Commit**

```bash
git add tray/src/ComposerToolbar.h tray/src/ComposerToolbar.cpp tray/tests/test_composertoolbar.cpp
git commit --only -m "Add a fixed-height composer toolbar with chips and popovers" -- \
  tray/src/ComposerToolbar.h tray/src/ComposerToolbar.cpp tray/tests/test_composertoolbar.cpp \
  tray/CMakeLists.txt tray/tests/CMakeLists.txt
```

(If another session has hunks in a CMake file, stage only your hunk with `git apply --cached` against `git show HEAD:<file>` instead of naming the file in `--only`.)

---

### Task 2: Composer owns the toolbar and an attachments chip

**Files:**
- Modify: `tray/src/MessageComposer.h`, `tray/src/MessageComposer.cpp`
- Modify: `tray/tests/test_messagecomposer.cpp`, `tray/tests/test_sessionswindow.cpp:2219-2272` (tile counts)
- Modify: `tray/tests/CMakeLists.txt` — add `../src/ComposerToolbar.cpp` to `test_messagecomposer` and `test_sessionswindow`

**Interfaces:**
- Consumes: Task 1 (`ComposerToolbar`, `ToolbarChip`, `ChipPopover`, `ChipTone`), `AttachmentFiles::store(name, bytes)` (`AttachmentViewer.h`).
- Produces:
  - `ComposerToolbar *MessageComposer::toolbar() const`
  - `void MessageComposer::setInputVisible(bool)`, `bool MessageComposer::isInputVisible() const`
  - `std::function<bool(const QUrl &)> MessageComposer::openUrl` (defaults to `QDesktopServices::openUrl`)
  - object names `attachmentsChip`, `attachmentsPopover`, `attachmentRow`, `attachmentPreview`, `messageInputArea`, `removeAttachment`
  - `setSessionKey()` closes every popover in its toolbar.

- [ ] **Step 1: Write the failing tests**

In `tray/tests/test_messagecomposer.cpp` add includes:

```cpp
#include "AttachmentViewer.h"
#include "ComposerToolbar.h"
#include <QBuffer>
#include <QEnterEvent>
```

Add to the anonymous namespace:

```cpp
QByteArray pngBytes()
{
    QImage image(8, 6, QImage::Format_RGB32); image.fill(Qt::darkCyan);
    QByteArray data; QBuffer buffer(&data); buffer.open(QIODevice::WriteOnly); image.save(&buffer, "PNG"); return data;
}

struct Window {
    QWidget widget; MessageComposer *composer = new MessageComposer;
    explicit Window(const QString &key) {
        auto *layout = new QVBoxLayout(&widget); layout->addWidget(composer);
        composer->setSessionKey(key); composer->setAvailability(true); widget.resize(720, 260); widget.show();
    }
    ToolbarChip *chip() const { return composer->findChild<ToolbarChip *>("attachmentsChip"); }
};
```

Declare the new slots after `draftStateFollowsUnsentContent();`:

```cpp
    void attachmentsChipSummarizesAndRemoves();
    void attachmentsPopoverFollowsSessionAndSending();
    void attachmentRowOpensStoredCopyAndPreviews();
    void hiddenInputKeepsToolbar();
```

Add the tests:

```cpp
void TestMessageComposer::attachmentsChipSummarizesAndRemoves()
{
    Window window("local/chip"); QVERIFY(QTest::qWaitForWindowExposed(&window.widget));
    auto *chip = window.chip(); QVERIFY(chip); QVERIFY(chip->isHidden());
    QVERIFY(window.composer->addAttachment("one.png", "image/png", pngBytes()));
    QVERIFY(window.composer->addAttachment("notes.md", "text/markdown", "# notes"));
    QVERIFY(chip->isVisible()); QCOMPARE(chip->fullLabel(), QString("2 attached")); QCOMPARE(chip->shortLabel(), QString("2"));
    QVERIFY(chip->property("flashing").toBool());
    QVERIFY(chip->toolTip().contains("[Image #1] one.png")); QVERIFY(chip->toolTip().contains("[File #2] notes.md"));
    QCOMPARE(window.composer->editor()->toPlainText(), QString("[Image #1] [File #2] "));
    QTest::mouseClick(chip, Qt::LeftButton); QTRY_VERIFY(chip->popover()->isVisible());
    QCOMPARE(chip->popover()->findChildren<QWidget *>("attachmentRow").size(), 2);
    chip->popover()->findChildren<QPushButton *>("removeAttachment").first()->click();
    QCOMPARE(chip->fullLabel(), QString("1 attached")); QVERIFY(chip->popover()->isVisible());
    QVERIFY(!window.composer->editor()->toPlainText().contains("[Image #1]"));
    QCOMPARE(chip->popover()->findChildren<QWidget *>("attachmentRow").size(), 1);
    chip->popover()->findChildren<QPushButton *>("removeAttachment").first()->click();
    QVERIFY(chip->isHidden()); QVERIFY(!chip->popover()->isVisible());
    QVERIFY(window.composer->findChildren<QPushButton *>("removeAttachment").isEmpty());
}

void TestMessageComposer::attachmentsPopoverFollowsSessionAndSending()
{
    Window window("local/a"); QVERIFY(QTest::qWaitForWindowExposed(&window.widget));
    QVERIFY(window.composer->addAttachment("a.txt", "text/plain", "a"));
    window.composer->setSessionKey("local/b");
    QVERIFY(window.composer->addAttachment("b.txt", "text/plain", "b")); QVERIFY(window.composer->addAttachment("c.txt", "text/plain", "c"));
    auto *chip = window.chip(); QTest::mouseClick(chip, Qt::LeftButton); QTRY_VERIFY(chip->popover()->isVisible());
    // Switching session never shows the previous session's list.
    window.composer->setSessionKey("local/a");
    QVERIFY(!chip->popover()->isVisible()); QCOMPARE(chip->fullLabel(), QString("1 attached"));
    QTest::mouseClick(chip, Qt::LeftButton); QTRY_VERIFY(chip->popover()->isVisible());
    QVERIFY(window.composer->setSending("local/a"));
    QVERIFY(!chip->popover()->isVisible()); QVERIFY(chip->isHidden());
    window.composer->deliveryFinished("local/a", false, "Network down");
    QVERIFY(chip->isVisible()); QCOMPARE(chip->fullLabel(), QString("1 attached"));
}

void TestMessageComposer::attachmentRowOpensStoredCopyAndPreviews()
{
    Window window("local/open"); QVERIFY(QTest::qWaitForWindowExposed(&window.widget));
    QList<QUrl> opened; window.composer->openUrl = [&opened](const QUrl &url) { opened << url; return true; };
    const QByteArray png = pngBytes(); QVERIFY(window.composer->addAttachment("shot.png", "image/png", png));
    auto *chip = window.chip(); QTest::mouseClick(chip, Qt::LeftButton); QTRY_VERIFY(chip->popover()->isVisible());
    auto *row = chip->popover()->findChild<QWidget *>("attachmentRow"); QVERIFY(row);
    QEnterEvent enter(QPointF(4, 4), row->mapToGlobal(QPointF(4, 4)), row->mapToGlobal(QPointF(4, 4)));
    QApplication::sendEvent(row, &enter);
    auto *preview = window.composer->findChild<QLabel *>("attachmentPreview");
    QVERIFY(preview->isVisible()); QVERIFY(!preview->pixmap().isNull());
    QEvent leave(QEvent::Leave); QApplication::sendEvent(row, &leave); QVERIFY(preview->isHidden());
    QTest::mouseClick(row, Qt::LeftButton, Qt::NoModifier, QPoint(row->width() / 2, row->height() / 2));
    QTRY_COMPARE(opened.size(), 1); QVERIFY(opened[0].isLocalFile());
    QFile stored(opened[0].toLocalFile()); QVERIFY(stored.open(QIODevice::ReadOnly)); QCOMPARE(stored.readAll(), png);
    QVERIFY(!chip->popover()->isVisible());
    QVERIFY(QDir(AttachmentFiles::root()).removeRecursively());
}

void TestMessageComposer::hiddenInputKeepsToolbar()
{
    Window window("local/question"); QVERIFY(QTest::qWaitForWindowExposed(&window.widget));
    QVERIFY(window.composer->isInputVisible());
    window.composer->setInputVisible(false);
    QVERIFY(!window.composer->isInputVisible()); QVERIFY(window.composer->isVisible());
    QVERIFY(window.composer->toolbar()->isVisible());
    QVERIFY(!window.composer->editor()->isVisible()); QVERIFY(!sendButton(*window.composer)->isVisible());
    window.composer->setInputVisible(true);
    QVERIFY(window.composer->editor()->isVisible()); QVERIFY(sendButton(*window.composer)->isVisible());
}
```

In `contentScaleEnlargesOnlyTheMessageField`, after `QCOMPARE(QFontInfo(send->font()).pixelSize(), sendFont);` add:

```cpp
    QCOMPARE(composer->toolbar()->height(), ComposerToolbar::RowHeight);
```

In `tray/tests/CMakeLists.txt` change the two source lists:

```cmake
add_executable(test_messagecomposer test_messagecomposer.cpp ../src/MessageComposer.cpp ../src/ComposerToolbar.cpp ../src/ComposerDraftStore.cpp)
```

and in the `test_sessionswindow` list replace `../src/MessageComposer.cpp ../src/ComposerDraftStore.cpp` with `../src/MessageComposer.cpp ../src/ComposerToolbar.cpp ../src/ComposerDraftStore.cpp`.

In `tray/tests/test_sessionswindow.cpp` replace every `findChildren<QWidget *>("attachmentTile")` (lines ~2219, 2228, 2234, 2235, 2247, 2248, 2267, 2272) with `findChildren<QWidget *>("attachmentRow")`. Rows exist while the popover is closed, so the counts stay the same.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cmake --build tray/build --target test_messagecomposer -j16`
Expected: compile FAILS — `no member named 'toolbar' in 'MessageComposer'`.

- [ ] **Step 3: Update the header**

In `tray/src/MessageComposer.h`:
- Add `#include <functional>` after `#include <QWidget>`.
- Replace the forward declaration `class QScrollArea;` with:

```cpp
class ComposerToolbar;
class ToolbarChip;
class QUrl;
class QVBoxLayout;
```

- After `QPlainTextEdit *editor() const { return m_editor; }` add:

```cpp
    // Notices and attachments above the field; the window adds its own chips.
    ComposerToolbar *toolbar() const { return m_toolbar; }
    // A required question replaces the field and its actions; the toolbar stays.
    void setInputVisible(bool visible);
    bool isInputVisible() const;
    // Opens a stored copy of an attachment; tests replace the desktop handler.
    std::function<bool(const QUrl &)> openUrl;
```

- Under `protected:` add `bool eventFilter(QObject *watched, QEvent *event) override;`
- Under `private:` after `void rebuildAttachments();` add:

```cpp
    void removeAttachment(int index);
    void openAttachment(int index);
    void showAttachmentPreview(QWidget *row, int index);
```

- Replace the members

```cpp
    QScrollArea *m_attachmentScroll;
    QWidget *m_attachmentList;
    QHBoxLayout *m_attachmentsLayout;
```

with

```cpp
    ComposerToolbar *m_toolbar;
    ToolbarChip *m_attachmentsChip;
    QWidget *m_attachmentList, *m_input;
    QVBoxLayout *m_attachmentsLayout;
    QLabel *m_preview;
```

- [ ] **Step 4: Build the toolbar into the composer**

In `tray/src/MessageComposer.cpp`:

Add includes after `#include "MessageComposer.h"`:

```cpp
#include "AttachmentViewer.h"
#include "ComposerToolbar.h"
```

and `#include <QDesktopServices>`, `#include <QMouseEvent>` among the Qt includes.

Delete the whole `class SettingsPopup : public QFrame { … };` from the anonymous namespace (it moved to `ChipPopover`), and add this helper there instead:

```cpp
// Decodes at most bounds; oversized or unreadable images give a null image.
QImage attachmentImage(const QByteArray &data, const QSize &bounds)
{
    QBuffer source; source.setData(data); source.open(QIODevice::ReadOnly);
    QImageReader reader(&source);
    const QSize size = reader.size();
    if (!size.isValid() || qint64(size.width()) * size.height() > 32000000) return {};
    reader.setScaledSize(size.scaled(bounds, Qt::KeepAspectRatio));
    return reader.read();
}
```

In the constructor replace

```cpp
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 8, 0, 0); layout->setSpacing(7);
    m_attachmentScroll = new QScrollArea; m_attachmentScroll->setObjectName("messageAttachments");
    m_attachmentScroll->setWidgetResizable(true); m_attachmentScroll->setFrameShape(QFrame::NoFrame);
    m_attachmentScroll->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_attachmentScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded); m_attachmentScroll->setFixedHeight(78);
    m_attachmentList = new QWidget; m_attachmentsLayout = new QHBoxLayout(m_attachmentList);
    m_attachmentsLayout->setContentsMargins(0, 0, 0, 0); m_attachmentsLayout->setSpacing(8);
    m_attachmentScroll->setWidget(m_attachmentList); m_attachmentScroll->hide(); layout->addWidget(m_attachmentScroll);
```

with

```cpp
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 4, 0, 0); layout->setSpacing(0);
    m_toolbar = new ComposerToolbar(this); layout->addWidget(m_toolbar);
    m_attachmentsChip = new ToolbarChip; m_attachmentsChip->setObjectName("attachmentsChip");
    m_attachmentsChip->setTone(ChipTone::Neutral); m_attachmentsChip->setIconName("attachment");
    m_attachmentList = new QWidget; m_attachmentList->setObjectName("attachmentsPopover");
    m_attachmentsLayout = new QVBoxLayout(m_attachmentList); m_attachmentsLayout->setContentsMargins(8, 8, 8, 8); m_attachmentsLayout->setSpacing(2);
    m_attachmentsChip->setPopoverContent(m_attachmentList);
    m_toolbar->add(ComposerToolbar::Slot::Attachments, m_attachmentsChip);
    m_preview = new QLabel(this, Qt::ToolTip); m_preview->setObjectName("attachmentPreview"); m_preview->hide();
    openUrl = [](const QUrl &url) { return QDesktopServices::openUrl(url); };
    m_input = new QWidget(this); m_input->setObjectName("messageInputArea");
    auto *inputLayout = new QVBoxLayout(m_input); inputLayout->setContentsMargins(0, 0, 0, 0); inputLayout->setSpacing(7);
    layout->addWidget(m_input);
```

Then, in the same constructor:
- `layout->addWidget(edit);` → `inputLayout->addWidget(edit);`
- `layout->addLayout(actions);` → `inputLayout->addLayout(actions);`
- `m_feedback = new QHBoxLayout; m_feedback->setContentsMargins(0, 0, 0, 0); m_feedback->setSpacing(8); layout->addLayout(m_feedback);` → same but `inputLayout->addLayout(m_feedback);`
- `m_settingsPopup = new SettingsPopup(this); m_settingsPopup->setObjectName("sessionSettingsPopup");` → `m_settingsPopup = new ChipPopover(this); m_settingsPopup->setObjectName("sessionSettingsPopup");`

In `setSessionKey`, after `m_settingsPopup->hide();` add:

```cpp
    m_toolbar->closePopovers(); m_preview->hide();
```

- [ ] **Step 5: Replace the tiles with popover rows**

Replace the whole body of `MessageComposer::rebuildAttachments()` with:

```cpp
void MessageComposer::rebuildAttachments()
{
    m_preview->hide();
    // Detach before deleting: a row's remove button may be the caller.
    while (auto *item = m_attachmentsLayout->takeAt(0)) {
        if (auto *row = item->widget()) { row->hide(); row->setParent(nullptr); row->deleteLater(); }
        delete item;
    }
    const auto draft = m_drafts.value(m_key);
    const int count = draft.sending ? 0 : int(draft.attachments.size());
    QStringList names;
    for (int i = 0; i < count; ++i) {
        const auto &attachment = draft.attachments[i];
        names << attachment.reference + ' ' + attachment.name;
        auto *row = new QWidget; row->setObjectName("attachmentRow"); row->setProperty("attachmentIndex", i);
        row->setAttribute(Qt::WA_StyledBackground); row->setAttribute(Qt::WA_Hover); row->setFocusPolicy(Qt::TabFocus);
        row->setCursor(Qt::PointingHandCursor); row->setToolTip(tr("Open %1").arg(attachment.name)); row->installEventFilter(this);
        auto *line = new QHBoxLayout(row); line->setContentsMargins(6, 4, 4, 4); line->setSpacing(8);
        auto *image = new QLabel; image->setFixedSize(30, 22); image->setAlignment(Qt::AlignCenter);
        const QImage thumbnail = attachmentImage(attachment.data, QSize(60, 44));
        image->setPixmap(thumbnail.isNull() ? style()->standardIcon(QStyle::SP_FileIcon).pixmap(18, 18)
            : QPixmap::fromImage(thumbnail).scaled(30, 22, Qt::KeepAspectRatio, Qt::SmoothTransformation));
        auto *text = new QLabel; text->setTextFormat(Qt::PlainText);
        text->setText(attachment.reference + ' ' + text->fontMetrics().elidedText(attachment.name, Qt::ElideMiddle, 180)
            + '\n' + tr("%1 KiB").arg(qMax<qsizetype>(1, (attachment.data.size() + 1023) / 1024)));
        auto *remove = new QPushButton(QStringLiteral("×")); remove->setObjectName("removeAttachment"); remove->setFixedSize(24, 24);
        remove->setAccessibleName(tr("Remove attachment %1").arg(attachment.name));
        connect(remove, &QPushButton::clicked, this, [this, i] { removeAttachment(i); });
        line->addWidget(image); line->addWidget(text, 1); line->addWidget(remove);
        m_attachmentsLayout->addWidget(row);
    }
    m_attachmentsChip->setLabels(tr("%1 attached").arg(count), QString::number(count));
    m_attachmentsChip->setDetail(names.join('\n'));
    m_attachmentsChip->setActive(count > 0);   // closes the popover with the last one
    if (m_attachmentsChip->popover()->isVisible()) m_attachmentsChip->popover()->reposition();
}

void MessageComposer::removeAttachment(int index)
{
    auto &current = m_drafts[m_key]; if (current.sending || index < 0 || index >= current.attachments.size()) return;
    const auto reference = current.attachments.takeAt(index).reference;
    current.attachmentHashes.clear();
    if (!reference.isEmpty()) {
        auto edit = m_editor->textCursor(); edit.beginEditBlock();
        auto match = m_editor->document()->find(reference);
        while (!match.isNull()) {
            match.removeSelectedText();
            match = m_editor->document()->find(reference, match);
        }
        edit.endEditBlock();
    }
    saveDraft(m_key); rebuildAttachments(); updateControls();
}

void MessageComposer::openAttachment(int index)
{
    const auto draft = m_drafts.value(m_key); if (draft.sending || index < 0 || index >= draft.attachments.size()) return;
    const auto &attachment = draft.attachments[index];
    const QString path = AttachmentFiles::store(attachment.name, attachment.data);
    if (path.isEmpty()) { showError(tr("Could not open %1.").arg(attachment.name)); return; }
    m_attachmentsChip->closePopover();
    // Wayland launches only after the popup has gone: dispatch once it closed.
    QTimer::singleShot(0, this, [this, path] {
        if (!openUrl(QUrl::fromLocalFile(path))) showError(tr("Could not open the attachment with its default application."));
    });
}

void MessageComposer::showAttachmentPreview(QWidget *row, int index)
{
    const auto draft = m_drafts.value(m_key); if (index < 0 || index >= draft.attachments.size()) return;
    const QImage image = attachmentImage(draft.attachments[index].data, QSize(320, 240));
    if (image.isNull()) { m_preview->hide(); return; }
    m_preview->setPixmap(QPixmap::fromImage(image)); m_preview->adjustSize();
    const QRect screen = row->screen()->availableGeometry();
    QPoint position = row->mapToGlobal(QPoint(row->width() + 10, 0));
    if (position.x() + m_preview->width() > screen.right()) position.setX(row->mapToGlobal(QPoint(0, 0)).x() - m_preview->width() - 10);
    position.setY(qBound(screen.top() + 8, position.y(), screen.bottom() - m_preview->height() - 8));
    m_preview->move(position); m_preview->show();
}

bool MessageComposer::eventFilter(QObject *watched, QEvent *event)
{
    auto *row = qobject_cast<QWidget *>(watched);
    if (row && row->objectName() == "attachmentRow") {
        const int index = row->property("attachmentIndex").toInt();
        switch (event->type()) {
        case QEvent::Enter: showAttachmentPreview(row, index); break;
        case QEvent::Leave: m_preview->hide(); break;
        case QEvent::MouseButtonPress: return true;   // take the grab so the release comes here
        case QEvent::MouseButtonRelease:
            if (static_cast<QMouseEvent *>(event)->button() == Qt::LeftButton) { openAttachment(index); return true; }
            break;
        case QEvent::KeyPress: {
            const int key = static_cast<QKeyEvent *>(event)->key();
            if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Space) { openAttachment(index); return true; }
            break;
        }
        default: break;
        }
    }
    return QWidget::eventFilter(watched, event);
}

void MessageComposer::setInputVisible(bool visible)
{
    if (!visible) m_settingsPopup->hide();
    m_input->setVisible(visible);
}

bool MessageComposer::isInputVisible() const { return m_input->isVisibleTo(this); }
```

In `addAttachment`, change the final line to flash the chip:

```cpp
    saveDraft(m_key); rebuildAttachments(); updateControls(); m_attachmentsChip->flash(); return true;
```

In `setTheme`, add `m_toolbar->setTheme(dark);` after `updateSettingsButton();`, and in the stylesheet replace the three lines

```
        QScrollArea#messageAttachments, QWidget#messageComposer { background:transparent; border:0; }
        QWidget#attachmentTile { background:%1; border:1px solid %3; border-radius:8px; }
        QWidget#attachmentTile QLabel { font-size:10px; border:0; }
        QPushButton#removeAttachment { padding:0; border:0; background:transparent; }
```

with

```
        QWidget#messageComposer { background:transparent; border:0; }
        QWidget#attachmentRow { border-radius:6px; }
        QWidget#attachmentRow:hover, QWidget#attachmentRow:focus { background:%4; }
        QWidget#attachmentsPopover QLabel { font-size:11px; border:0; background:transparent; }
        QPushButton#removeAttachment { padding:0; border:0; background:transparent; font-size:15px; color:%7; }
        QPushButton#removeAttachment:hover { color:%2; }
        QLabel#attachmentPreview { background:%1; border:1px solid %3; border-radius:6px; padding:4px; }
```

- [ ] **Step 6: Run the composer and window tests**

Run: `cmake --build tray/build --target test_messagecomposer test_sessionswindow -j16 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_messagecomposer`
Expected: all pass, including the four new functions.

Run: `ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: 10/10 shards pass.

- [ ] **Step 7: Commit**

```bash
git commit --only -m "Show composer attachments as one toolbar chip with a popover" -- \
  tray/src/MessageComposer.h tray/src/MessageComposer.cpp tray/tests/test_messagecomposer.cpp \
  tray/tests/test_sessionswindow.cpp tray/tests/CMakeLists.txt
```

---

### Task 3: The toolbar replaces the activity footer

**Files:**
- Modify: `tray/src/SessionsWindow.h`, `tray/src/SessionsWindow.cpp` (footer block ~620-669, theme ~1265-1271, `renderDetails` ~2329-2332, question ~3029)
- Modify: `tray/src/SessionUsage.h:13`
- Modify: `tray/tests/test_sessionswindow.cpp` (question tests ~1655-1955, Mark as read test ~2174-2191)

**Interfaces:**
- Consumes: `MessageComposer::toolbar()`, `setInputVisible`, `isInputVisible` (Task 2); `ComposerToolbar::Slot`, `ToolbarChip` (Task 1).
- Produces: `ToolbarChip *SessionsWindow::m_markRead` (object name `activityMarkRead`); `m_cacheStatus` and `m_contextUsage` live in the `Cache` and `Context` slots; Jump to latest is an Activity overlay again.

- [ ] **Step 1: Update the tests first**

In `tray/tests/test_sessionswindow.cpp`, question tests now mean "input hidden" rather than "composer hidden":
- line ~1655: `QVERIFY(!composer->isVisible());` → `QVERIFY(!composer->isInputVisible()); QVERIFY(composer->toolbar()->isVisible());`
- lines ~1678, 1727, 1828, 1875, 1888, 1955: `composer->isVisible()` → `composer->isInputVisible()`
- lines ~1869, 1872: `composer->isHidden()` → `!composer->isInputVisible()`
- Leave lines ~2780 and ~2829 (`!composer->isVisible()` while the subagent page is shown) unchanged: the whole activity page is hidden there.

Add `#include "ComposerToolbar.h"` to the test's includes.

In `activityMarkReadKeepsReadingPosition` (~2174-2191) replace

```cpp
    auto *latest=window.findChild<ActivityView *>("mainActivity")->jumpButton();QVERIFY(latest->isVisible());
    const auto bounds=[&](QWidget *widget){return QRect(widget->mapTo(&window,QPoint()),widget->size());};
    QVERIFY(bounds(latest).top()>bounds(browser).bottom());
    QVERIFY(qAbs(bounds(latest).center().y()-bounds(button).center().y())<=1);
    QVERIFY(qAbs(bounds(latest).center().x()-bounds(composer).center().x())<=1);
    QVERIFY(bounds(button).right()<bounds(latest).left());QVERIFY(bounds(latest).right()<bounds(context).left());
    QCOMPARE(browser->geometry(),initialGeometry);QCOMPARE(latest->height(),24);
```

with

```cpp
    auto *latest=window.findChild<ActivityView *>("mainActivity")->jumpButton();QVERIFY(latest->isVisible());
    const auto bounds=[&](QWidget *widget){return QRect(widget->mapTo(&window,QPoint()),widget->size());};
    // Jump to latest floats over the journal; Mark as read and the context share the toolbar row.
    QCOMPARE(latest->parentWidget(),browser);QVERIFY(bounds(browser).contains(bounds(latest)));
    QVERIFY(qAbs(bounds(button).center().y()-bounds(context).center().y())<=1);
    QVERIFY(bounds(button).top()>bounds(browser).bottom());QVERIFY(bounds(button).right()<bounds(context).left());
    QCOMPARE(browser->geometry(),initialGeometry);
```

and later in the same test replace

```cpp
    QVERIFY(qAbs(bounds(latest).center().x()-bounds(composer).center().x())<=1);
```

with

```cpp
    QVERIFY(!latest->isVisible() || bounds(browser).contains(bounds(latest)));
```

- [ ] **Step 2: Run the changed tests to verify they fail**

Run: `cmake --build tray/build --target test_sessionswindow -j16 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_sessionswindow activityMarkReadKeepsReadingPosition`
Expected: builds (Task 2 added `isInputVisible`), then FAILS at `QCOMPARE(latest->parentWidget(),browser)` because the button still sits in the footer.

- [ ] **Step 3: Put Mark as read, compaction, cache and context into the toolbar**

In `tray/src/SessionsWindow.h` add the forward declaration `class ToolbarChip;` next to the other forward declarations and change `QPushButton *m_markRead;` to `ToolbarChip *m_markRead;`.

In `tray/src/SessionsWindow.cpp` add `#include "ComposerToolbar.h"` with the other project includes.

Replace this block (from `auto *activityFooter=new QGridLayout;` through `auto *latest=m_activityView->jumpButton();latest->setProperty("footer",true);latest->setFixedHeight(24);` and `activityFooter->addWidget(latest,0,1,Qt::AlignCenter);`):

```cpp
    auto *activityFooter=new QGridLayout;activityFooter->setContentsMargins(0,8,0,0);activityFooter->setSpacing(8);
    activityFooter->setColumnStretch(0,1);activityFooter->setColumnStretch(2,1);
    activityFooter->setRowMinimumHeight(0,24);
    auto *leftStatus=new QWidget;auto *rightStatus=new QWidget;
    auto *footerLeft=new QHBoxLayout(leftStatus);footerLeft->setContentsMargins(14,0,0,0);footerLeft->setSpacing(8);
    auto *footerRight=new QHBoxLayout(rightStatus);footerRight->setContentsMargins(0,0,0,0);footerRight->setSpacing(8);
    activityFooter->addWidget(leftStatus,0,0);activityFooter->addWidget(rightStatus,0,2);
    auto *latest=m_activityView->jumpButton();latest->setProperty("footer",true);latest->setFixedHeight(24);
    activityFooter->addWidget(latest,0,1,Qt::AlignCenter);
```

with

```cpp
    auto *toolbar=m_composer->toolbar();
```

Replace

```cpp
    m_markRead=new QPushButton(tr("Mark as read"));m_markRead->setObjectName("activityMarkRead");
    m_markRead->setFixedHeight(24);m_markRead->setIconSize(QSize(14,14));m_markRead->setCursor(Qt::PointingHandCursor);
    m_markRead->setFocusPolicy(Qt::TabFocus);m_markRead->hide();footerLeft->addWidget(m_markRead);
    auto *compaction=m_activityView->compactionIndicator();
    compaction->layout()->setContentsMargins(0,0,0,0);footerLeft->addWidget(compaction);footerLeft->addWidget(m_compactCancel);footerLeft->addStretch();
    footerRight->addStretch();m_cacheStatus=new CacheStatus::Button;m_cacheStatus->setSizePolicy(QSizePolicy::Preferred,QSizePolicy::Fixed);
    footerRight->addWidget(m_cacheStatus);footerRight->addWidget(m_contextUsage);
```

with

```cpp
    m_markRead=new ToolbarChip;m_markRead->setObjectName("activityMarkRead");
    m_markRead->setLabels(tr("Mark as read"));m_markRead->setIconName("read-all");m_markRead->setTone(ChipTone::Quiet);
    toolbar->add(ComposerToolbar::Slot::MarkRead,m_markRead);
    auto *compaction=m_activityView->compactionIndicator();compaction->layout()->setContentsMargins(0,0,0,0);
    toolbar->add(ComposerToolbar::Slot::Compaction,compaction);toolbar->add(ComposerToolbar::Slot::CompactionCancel,m_compactCancel);
    m_cacheStatus=new CacheStatus::Button;m_cacheStatus->setSizePolicy(QSizePolicy::Preferred,QSizePolicy::Fixed);
    toolbar->add(ComposerToolbar::Slot::Cache,m_cacheStatus);toolbar->add(ComposerToolbar::Slot::Context,m_contextUsage);
```

Replace `activityLayout->addLayout(activityFooter);activityLayout->addWidget(m_composer);` with `activityLayout->addWidget(m_composer);`.

In `applyTheme` (~1265-1271) delete these lines (the toolbar themes the chip):

```cpp
    const auto readColor=m_dark?QString("#8bdfc0"):QString("#167357");
    m_markRead->setIcon(workspaceIcon("read-all",QColor(readColor)));
    m_markRead->setStyleSheet(QString("QPushButton {color:%1;background:%2;border:1px solid %3;border-radius:11px;padding:0 10px;min-height:0;font-size:11px;} QPushButton:hover,QPushButton:focus[keyboardFocus=\"true\"] {border-color:%1;}")
        .arg(readColor,m_dark?"#233a35":"#e7f3ed",m_dark?"#456e61":"#a5c8b8"));
    m_markRead->setFixedHeight(24);
```

In `renderDetails` (~2329) replace

```cpp
    m_markRead->setVisible(entry && entry->session.state!="archived" && entryNeedsAttention(*entry));
    m_markRead->setToolTip(entry && entry->session.reviewLater
```

with

```cpp
    m_markRead->setActive(entry && entry->session.state!="archived" && entryNeedsAttention(*entry));
    m_markRead->setDetail(entry && entry->session.reviewLater
```

(the two `tr(...)` branches that follow stay as they are).

In the question code (~3029) replace

```cpp
    m_composer->setVisible(!available || question.value("optional").toBool());
```

with

```cpp
    m_composer->setInputVisible(!available || question.value("optional").toBool());
```

In `tray/src/SessionUsage.h`, in `ContextButton`'s constructor, replace `setFixedSize(220,24);` with:

```cpp
        setFixedHeight(24);setSizePolicy(QSizePolicy::Fixed,QSizePolicy::Fixed);
```

- [ ] **Step 4: Run the window suite**

Run: `cmake --build tray/build --target test_sessionswindow -j16 && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: 10/10 pass. The compaction test at ~1026 (indicator centred with `activityContext`), the cancel-send test at ~1105 and the question-footer test at ~1996 (Mark as read and context at least 8 px below the card) must pass unchanged.

Run: `QT_QPA_PLATFORM=offscreen tray/build/tests/test_activityview` (build it first with `--target test_activityview`)
Expected: all pass (overlay positioning is ActivityView's own behaviour).

- [ ] **Step 5: Commit**

```bash
git commit --only -m "Move Mark as read, compaction, cache and context into the composer toolbar" -- \
  tray/src/SessionsWindow.h tray/src/SessionsWindow.cpp tray/src/SessionUsage.h tray/tests/test_sessionswindow.cpp
```

---

### Task 4: Subagent page keeps a toolbar

**Files:**
- Modify: `tray/src/SessionsWindow.cpp` (~690-692, ~888, ~1050, ~2266)
- Modify: `tray/tests/test_sessionswindow.cpp` (~2244, ~2786)

**Interfaces:**
- Consumes: `MessageComposer::toolbar()`, `setInputVisible`, `isInputVisible` (Task 2).
- Produces: `subagentContext` lives in the subagent composer's toolbar; the subagent composer is always visible on its page.

- [ ] **Step 1: Write the failing assertions**

In the test that opens `child-1` read-only (~2786), after `QCOMPARE(window.findChild<QPushButton *>("subagentContext")->property("contextPercent").toDouble(),25.);` add:

```cpp
    auto *childComposer=window.findChild<MessageComposer *>("subagentComposer");auto *childContext=window.findChild<QPushButton *>("subagentContext");
    QVERIFY(childComposer->isVisible());QVERIFY(!childComposer->isInputVisible());QVERIFY(childContext->isVisible());
    QVERIFY(childComposer->toolbar()->isAncestorOf(childContext));
```

At ~2244 replace `QVERIFY(child->isVisible());` with `QVERIFY(child->isInputVisible());`.

- [ ] **Step 2: Run to verify it fails**

Run: `cmake --build tray/build --target test_sessionswindow -j16 && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: one shard FAILS at `QVERIFY(childComposer->isVisible())`.

- [ ] **Step 3: Implement**

In `tray/src/SessionsWindow.cpp` replace

```cpp
    m_subagentComposer = new MessageComposer; m_subagentComposer->setObjectName("subagentComposer"); m_subagentComposer->hide();
    m_subagentContextUsage=new SessionUsage::ContextButton;m_subagentContextUsage->setObjectName("subagentContext");
    subagentLayout->addWidget(m_subagentContextUsage,0,Qt::AlignRight);
```

with

```cpp
    m_subagentComposer = new MessageComposer; m_subagentComposer->setObjectName("subagentComposer"); m_subagentComposer->setInputVisible(false);
    m_subagentContextUsage=new SessionUsage::ContextButton;m_subagentContextUsage->setObjectName("subagentContext");
    m_subagentComposer->toolbar()->add(ComposerToolbar::Slot::Context,m_subagentContextUsage);
```

Then:
- `return m_subagentComposer->isVisible()?m_subagentComposer:nullptr;` → `return m_subagentComposer->isInputVisible()?m_subagentComposer:nullptr;`
- `m_subagentComposer->setVisible(data.value("send_supported").toBool());` → `m_subagentComposer->setInputVisible(data.value("send_supported").toBool());`
- `m_subagentDetails = {}; m_subagentComposer->hide();` → `m_subagentDetails = {}; m_subagentComposer->setInputVisible(false);`

- [ ] **Step 4: Run the window suite**

Run: `cmake --build tray/build --target test_sessionswindow -j16 && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: 10/10 pass, including the drop test (~2240) that routes a drop to a visible, sendable subagent composer.

- [ ] **Step 5: Commit**

```bash
git commit --only -m "Keep the subagent context counter in the subagent composer toolbar" -- \
  tray/src/SessionsWindow.cpp tray/tests/test_sessionswindow.cpp
```

---

### Task 5: Cache chip with a Clear context popover

**Files:**
- Modify: `tray/src/CacheStatus.h` (replace `Button`)
- Modify: `tray/src/SessionsWindow.h`, `tray/src/SessionsWindow.cpp` (cache block ~650-669, theme ~1262-1264, `renderDetails` ~2341-2344)
- Modify: `tray/tests/test_accountusage.cpp`, `tray/tests/test_sessionswindow.cpp:~980`
- Modify: `tray/tests/CMakeLists.txt` — `test_accountusage` gains `../src/ComposerToolbar.cpp` (CacheStatus.h now includes the chip)

**Interfaces:**
- Consumes: `ToolbarChip`, `ChipTone` (Task 1); toolbar `Slot::Cache` (Task 3).
- Produces: `struct CacheStatus::ChipState { bool visible; ChipTone tone; QString label, shortLabel; }`, `CacheStatus::chipState(const QJsonObject &)`, `CacheStatus::details(const QJsonObject &, bool recorded)`, `class CacheStatus::Chip : ToolbarChip` with `setData(const QJsonObject &, bool recorded = false)`; object names `cacheChip`, `cachePopover`, `cacheWarning`, `cacheDetail`, `cacheUsageDetails`, `cacheClearContext`.

- [ ] **Step 1: Write the failing tests**

In `tray/tests/test_accountusage.cpp` add a slot:

```cpp
    void cacheChipStateFollowsHintAndCountdown() {
        QJsonObject data;
        QVERIFY(!CacheStatus::chipState(data).visible);
        data["session_usage"]=QJsonObject{{"prompt_cache",QJsonObject{{"status","warm"},{"expires_at",double(QDateTime::currentSecsSinceEpoch()+240)}}}};
        auto state=CacheStatus::chipState(data);QVERIFY(state.visible);QCOMPARE(state.tone,ChipTone::Success);
        QCOMPARE(state.label,QString("Cache ~4m"));QCOMPARE(state.shortLabel,QString("~4m"));
        data["cache_hint"]=QJsonObject{{"status","saving_hint"},{"tokens",484800}};
        state=CacheStatus::chipState(data);QCOMPARE(state.tone,ChipTone::Warning);
        QCOMPARE(state.label,QString("Clear suggested"));QCOMPARE(state.shortLabel,QString("/clear"));
        data["cache_hint"]=QJsonObject{{"status","cold"},{"tokens",756000}};
        state=CacheStatus::chipState(data);QCOMPARE(state.tone,ChipTone::Danger);
        QCOMPARE(state.label,QString("Cold cache"));QCOMPARE(state.shortLabel,QString("Cold"));
        QVERIFY(!CacheStatus::details(data,true).contains("cold cache"));QVERIFY(CacheStatus::details(data,true).contains("Last recorded"));
    }
```

In `tray/tests/CMakeLists.txt` change the `test_accountusage` sources to

```cmake
add_executable(test_accountusage test_accountusage.cpp ../src/AccountUsageStore.cpp ../src/HgsClient.cpp ../src/FleetState.cpp ../src/ComposerToolbar.cpp)
```

In `coldCacheClearKeepsDraftAndPinsIdentity` (~980) replace

```cpp
    auto *clear=window.findChild<QPushButton *>("clearContextFromCache");QVERIFY(clear->isVisible());QVERIFY(clear->isEnabled());QCOMPARE(clear->text(),QString("Clear context"));
```

with

```cpp
    auto *cacheChip=window.findChild<ToolbarChip *>("cacheChip");QVERIFY(cacheChip && cacheChip->isVisible());
    QCOMPARE(cacheChip->fullLabel(),QString("Cold cache"));QCOMPARE(cacheChip->tone(),ChipTone::Danger);
    QTest::mouseClick(cacheChip,Qt::LeftButton);QTRY_VERIFY(cacheChip->popover()->isVisible());
    QVERIFY(window.findChild<QLabel *>("cacheWarning")->text().contains("cold cache"));
    auto *clear=window.findChild<QPushButton *>("cacheClearContext");QVERIFY(clear->isVisible());QVERIFY(clear->isEnabled());QCOMPARE(clear->text(),QString("Clear context"));
```

The later `clear->click()` calls stay: clicking closes the popover first, then asks for confirmation as before.

- [ ] **Step 2: Run to verify they fail**

Run: `cmake --build tray/build --target test_accountusage -j16`
Expected: compile FAILS — `no member named 'chipState' in namespace 'CacheStatus'`.

- [ ] **Step 3: Rewrite `CacheStatus.h`**

Replace the includes and everything after `warning()` in `tray/src/CacheStatus.h` so the file reads:

```cpp
#pragma once
#include "ComposerToolbar.h"
#include <QDateTime>
#include <QJsonObject>
#include <QLocale>
#include <QTimer>
#include <cmath>

namespace CacheStatus {
inline bool expired(const QJsonObject &data) { /* unchanged */ }
inline QString warning(const QJsonObject &data) { /* unchanged */ }

struct ChipState { bool visible=false; ChipTone tone=ChipTone::Quiet; QString label,shortLabel; };
inline ChipState chipState(const QJsonObject &data) {
    const auto cache=data.value("session_usage").toObject().value("prompt_cache").toObject();
    const auto remaining=cache.value("expires_at").toDouble()-QDateTime::currentSecsSinceEpoch();
    if(expired(data))return {true,ChipTone::Danger,QObject::tr("Cold cache"),QObject::tr("Cold")};
    if(data.value("cache_hint").toObject().value("status")=="saving_hint")return {true,ChipTone::Warning,QObject::tr("Clear suggested"),QStringLiteral("/clear")};
    if(remaining>0&&cache.value("status")=="warm") {
        const auto minutes=qint64(std::ceil(remaining/60.));
        return {true,ChipTone::Success,QObject::tr("Cache ~%1m").arg(minutes),QObject::tr("~%1m").arg(minutes)};
    }
    return {};
}
// The former tooltip without the warning, which the popover shows on its own.
inline QString details(const QJsonObject &data,bool recorded) {
    const auto cache=data.value("session_usage").toObject().value("prompt_cache").toObject();QStringList detail;
    if(cache.value("expires_at").toDouble()>0)detail<<QObject::tr("Estimated expiry from the TTL reported in native usage: %1.").arg(QDateTime::fromSecsSinceEpoch(qint64(cache.value("expires_at").toDouble())).toLocalTime().toString("d MMM HH:mm:ss"));
    else detail<<QObject::tr("This agent has not reported a cache expiry. No countdown is available.");
    if(cache.value("cache_read").isDouble())detail<<QObject::tr("Last request read %1 tokens from cache.").arg(QLocale().toString(cache.value("cache_read").toInteger()));
    detail<<QObject::tr("Prompt, model or tool changes can invalidate the cache earlier. Starting a new conversation is always your choice.");
    if(recorded)detail<<QObject::tr("Last recorded session data.");
    return detail.join('\n');
}
// Ticks the warm-cache countdown once a second while it is shown.
class Chip:public ToolbarChip {
public:
    explicit Chip(QWidget *parent=nullptr):ToolbarChip(parent){
        setObjectName("cacheChip");
        auto *timer=new QTimer(this);timer->setInterval(1000);connect(timer,&QTimer::timeout,this,[this]{if(isActive())refresh();});timer->start();
    }
    void setData(const QJsonObject &value,bool recorded=false){m_data=value;m_recorded=recorded;refresh();}
private:
    void refresh(){
        const auto state=chipState(m_data);
        setLabels(state.label,state.shortLabel);setTone(state.tone);
        setIconName(state.tone==ChipTone::Success?QString():QStringLiteral("context-warning"));
        const auto note=warning(m_data);setDetail((note.isEmpty()?QString():note+'\n')+details(m_data,m_recorded));
        setActive(state.visible);
    }
    QJsonObject m_data;bool m_recorded=false;
};
}
```

(Keep `expired()` and `warning()` exactly as they are today; the `/* unchanged */` markers mean "do not edit these two bodies".)

- [ ] **Step 4: Build the cache popover in `SessionsWindow`**

In `tray/src/SessionsWindow.h`: change `namespace CacheStatus {class Button;}` to `namespace CacheStatus {class Chip;}`, `CacheStatus::Button *m_cacheStatus;` to `CacheStatus::Chip *m_cacheStatus;`, and replace `QWidget *m_cacheNotice;` with `QLabel *m_cacheDetail;`.

In `tray/src/SessionsWindow.cpp` replace (Task 3 left these lines in place)

```cpp
    m_cacheStatus=new CacheStatus::Button;m_cacheStatus->setSizePolicy(QSizePolicy::Preferred,QSizePolicy::Fixed);
    toolbar->add(ComposerToolbar::Slot::Cache,m_cacheStatus);toolbar->add(ComposerToolbar::Slot::Context,m_contextUsage);
    m_cacheWarning=new QLabel;m_cacheWarning->setObjectName("activityCacheWarning");m_cacheWarning->setWordWrap(true);m_cacheWarning->setTextFormat(Qt::PlainText);m_cacheWarning->hide();
    m_cacheNotice=new QWidget;m_cacheNotice->setObjectName("activityCacheNotice");
    auto *cacheRow=new QHBoxLayout(m_cacheNotice);cacheRow->setContentsMargins(0,0,0,0);cacheRow->setSpacing(8);cacheRow->addWidget(m_cacheWarning,1);
    m_cacheClear=new QPushButton(tr("Clear context"));m_cacheClear->setObjectName("clearContextFromCache");m_cacheClear->setAutoDefault(false);
    m_cacheClear->setToolTip(tr("Clear this agent’s conversation, like /clear in Terminal. Your unsent message stays here."));
    cacheRow->addWidget(m_cacheClear,0,Qt::AlignVCenter);m_cacheNotice->hide();activityLayout->addWidget(m_cacheNotice);
    connect(m_cacheClear,&QPushButton::clicked,this,&SessionsWindow::clearContext);
```

with

```cpp
    m_cacheStatus=new CacheStatus::Chip;
    auto *cachePanel=new QWidget;cachePanel->setObjectName("cachePopover");
    auto *cacheLayout=new QVBoxLayout(cachePanel);cacheLayout->setContentsMargins(14,12,14,12);cacheLayout->setSpacing(8);
    m_cacheWarning=new QLabel;m_cacheWarning->setObjectName("cacheWarning");m_cacheWarning->setWordWrap(true);m_cacheWarning->setTextFormat(Qt::PlainText);
    m_cacheDetail=new QLabel;m_cacheDetail->setObjectName("cacheDetail");m_cacheDetail->setWordWrap(true);m_cacheDetail->setTextFormat(Qt::PlainText);
    auto *cacheActions=new QHBoxLayout;cacheActions->setSpacing(8);cacheActions->addStretch();
    auto *cacheUsage=new QPushButton(tr("Usage details"));cacheUsage->setObjectName("cacheUsageDetails");cacheUsage->setAutoDefault(false);
    m_cacheClear=new QPushButton(tr("Clear context"));m_cacheClear->setObjectName("cacheClearContext");m_cacheClear->setAutoDefault(false);
    m_cacheClear->setToolTip(tr("Clear this agent’s conversation, like /clear in Terminal. Your unsent message stays here."));
    cacheActions->addWidget(cacheUsage);cacheActions->addWidget(m_cacheClear);
    cacheLayout->addWidget(m_cacheWarning);cacheLayout->addWidget(m_cacheDetail);cacheLayout->addLayout(cacheActions);
    m_cacheStatus->setPopoverContent(cachePanel);
    toolbar->add(ComposerToolbar::Slot::Cache,m_cacheStatus);toolbar->add(ComposerToolbar::Slot::Context,m_contextUsage);
    connect(cacheUsage,&QPushButton::clicked,this,[this]{m_cacheStatus->closePopover();setInspectorVisible(true);m_inspector->setCurrentIndex(1);});
    // Close first: the confirmation dialog must not open over a popup.
    connect(m_cacheClear,&QPushButton::clicked,this,[this]{m_cacheStatus->closePopover();clearContext();});
```

Delete the old `connect(m_cacheStatus,&QPushButton::clicked,this,[this]{setInspectorVisible(true);m_inspector->setCurrentIndex(1);});` (the popover's Usage details button replaces it).

In `applyTheme` replace

```cpp
    m_cacheWarning->setStyleSheet(QString("QLabel{color:%1;padding:6px 14px;font-size:11px;}").arg(m_dark?"#efbd78":"#91621a"));
```

with

```cpp
    m_cacheWarning->setStyleSheet(QString("QLabel{color:%1;font-size:12px;}").arg(m_dark?"#efbd78":"#91621a"));
    m_cacheDetail->setStyleSheet(QString("QLabel{color:%1;font-size:11px;}").arg(m_muted));
```

In `renderDetails` replace

```cpp
    const auto cacheWarning=entry?CacheStatus::warning(m_details):QString();m_cacheWarning->setText(cacheWarning);m_cacheWarning->setVisible(!cacheWarning.isEmpty());
    m_cacheNotice->setVisible(!cacheWarning.isEmpty());
```

with

```cpp
    const auto cacheWarning=entry?CacheStatus::warning(m_details):QString();m_cacheWarning->setText(cacheWarning);m_cacheWarning->setVisible(!cacheWarning.isEmpty());
    m_cacheDetail->setText(CacheStatus::details(m_details,recorded));m_cacheClear->setVisible(!cacheWarning.isEmpty());
```

(`m_cacheClear->setEnabled(...)` on the next line stays unchanged.)

- [ ] **Step 5: Run the tests**

Run: `cmake --build tray/build --target test_accountusage test_sessionswindow -j16 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_accountusage && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: all pass, including `coldCacheConfirmationPreservesDraftAndPinsConversation` (the send-time dialog is unchanged).

- [ ] **Step 6: Commit**

```bash
git commit --only -m "Turn the cache notice into a toolbar chip with a Clear context popover" -- \
  tray/src/CacheStatus.h tray/src/SessionsWindow.h tray/src/SessionsWindow.cpp \
  tray/tests/test_accountusage.cpp tray/tests/test_sessionswindow.cpp tray/tests/CMakeLists.txt
```

---

### Task 6: Usage-limit chip

**Files:**
- Modify: `tray/src/AccountUsage.h` (after `exhausted()`, ~line 60)
- Modify: `tray/src/SessionsWindow.h`, `tray/src/SessionsWindow.cpp` (~600, theme ~1259, `renderAccountUsage` ~2232-2240)
- Modify: `tray/tests/test_accountusage.cpp`, `tray/tests/test_sessionswindow.cpp` (new function)

**Interfaces:**
- Consumes: `ToolbarChip` (Task 1), `Slot::UsageLimit`.
- Produces: `QString AccountUsage::exhaustedSummary(const QJsonObject &)`; `ToolbarChip *SessionsWindow::m_usageLimit` (`usageLimitChip`), popover `usageLimitPopover` with `activityUsageWarning` and `usageLimitRefresh`.

- [ ] **Step 1: Write the failing tests**

In `tray/tests/test_accountusage.cpp` add:

```cpp
    void exhaustedSummaryNamesNearestReset() {
        const auto now=QDateTime::currentSecsSinceEpoch();
        QJsonObject data{{"windows",QJsonArray{QJsonObject{{"window_minutes",300},{"used_percent",80}}}}};
        QVERIFY(AccountUsage::exhaustedSummary(data).isEmpty());
        data["windows"]=QJsonArray{QJsonObject{{"window_minutes",10080},{"used_percent",100},{"resets_at",now+3*86400}},
            QJsonObject{{"window_minutes",300},{"used_percent",100},{"resets_at",now+2*3600+30}}};
        QCOMPARE(AccountUsage::exhaustedSummary(data),QString("Limit reached · 2h 1m"));
        data["windows"]=QJsonArray{QJsonObject{{"window_minutes",300},{"used_percent",100}}};
        QCOMPARE(AccountUsage::exhaustedSummary(data),QString("Limit reached"));
        data["windows"]=QJsonArray{QJsonObject{{"window_minutes",300},{"used_percent",100},{"resets_at",now-60}}};
        QCOMPARE(AccountUsage::exhaustedSummary(data),QString("Refresh usage"));
    }
```

In `tray/tests/test_sessionswindow.cpp` declare `void usageLimitBecomesToolbarChip();` in the class and add:

```cpp
void TestSessionsWindow::usageLimitBecomesToolbarChip()
{
    SessionsWindow window(script());window.resize(1100,760);window.setFleet(fleet());window.show();window.showSession({},"codex/hgs/dashboard");
    auto *client=window.findChild<AccountUsageStore *>()->findChild<HgsClient *>();
    auto *chip=window.findChild<ToolbarChip *>("usageLimitChip");QVERIFY(chip);QVERIFY(chip->isHidden());
    const auto now=QDateTime::currentSecsSinceEpoch();
    client->accountsReady(1,{},QJsonObject{{"id","native-codex"},{"status","ok"},{"windows",QJsonArray{
        QJsonObject{{"window_minutes",300},{"used_percent",100},{"resets_at",now+3600}}}}});
    QTRY_VERIFY(chip->isVisible());QVERIFY(chip->fullLabel().startsWith("Limit reached"));QCOMPARE(chip->tone(),ChipTone::Danger);
    QVERIFY(chip->toolTip().contains("Resets in"));
    QTest::mouseClick(chip,Qt::LeftButton);QTRY_VERIFY(chip->popover()->isVisible());
    QVERIFY(window.findChild<QLabel *>("activityUsageWarning")->text().contains("Resets in"));
    auto *refresh=window.findChild<QPushButton *>("usageLimitRefresh");QVERIFY(refresh->isVisible());refresh->click();
    QVERIFY(static_cast<AccountUsage::RefreshButton *>(window.findChild<QPushButton *>("sessionUsageRefresh"))->isRefreshing());
    // Switching session closes the popover rather than showing another account's limit.
    QTest::mouseClick(chip,Qt::LeftButton);QTRY_VERIFY(chip->popover()->isVisible());
    window.showSession("mac","claude/infra/review");QVERIFY(!chip->popover()->isVisible());
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cmake --build tray/build --target test_accountusage -j16`
Expected: compile FAILS — `no member named 'exhaustedSummary'`.

- [ ] **Step 3: Implement `exhaustedSummary`**

In `tray/src/AccountUsage.h`, directly after `exhausted()`:

```cpp
// Chip label: the nearest reset of an exhausted current window, or a reminder to
// refresh when only windows that already ended still report 100%.
inline QString exhaustedSummary(const QJsonObject &data) {
    QDateTime nearest;bool reached=false,ended=false;
    for(const auto &value:data.value("windows").toArray()){
        const auto w=value.toObject();if(w.value("used_percent").toDouble()<100)continue;
        if(!current(w)){ended=true;continue;}
        reached=true;const auto at=reset(w.value("resets_at"));
        if(at.isValid()&&(!nearest.isValid()||at<nearest))nearest=at;
    }
    if(reached)return nearest.isValid()?QObject::tr("Limit reached · %1").arg(remaining(nearest)):QObject::tr("Limit reached");
    return ended?QObject::tr("Refresh usage"):QString();
}
```

- [ ] **Step 4: Replace the banner with the chip**

In `tray/src/SessionsWindow.h` add `ToolbarChip *m_usageLimit;` next to `QLabel *m_usageWarning;`.

In `tray/src/SessionsWindow.cpp` replace

```cpp
    m_usageWarning=new QLabel;m_usageWarning->setObjectName("activityUsageWarning");m_usageWarning->setTextFormat(Qt::PlainText);m_usageWarning->setWordWrap(true);m_usageWarning->hide();activityLayout->addWidget(m_usageWarning);
```

with

```cpp
    m_usageLimit=new ToolbarChip;m_usageLimit->setObjectName("usageLimitChip");m_usageLimit->setTone(ChipTone::Danger);m_usageLimit->setIconName("attention");
    auto *usagePanel=new QWidget;usagePanel->setObjectName("usageLimitPopover");
    auto *usageLayout=new QVBoxLayout(usagePanel);usageLayout->setContentsMargins(14,12,14,12);usageLayout->setSpacing(8);
    m_usageWarning=new QLabel;m_usageWarning->setObjectName("activityUsageWarning");m_usageWarning->setTextFormat(Qt::PlainText);m_usageWarning->setWordWrap(true);
    auto *usageActions=new QHBoxLayout;usageActions->addStretch();
    auto *usageRefresh=new QPushButton(tr("Refresh usage"));usageRefresh->setObjectName("usageLimitRefresh");usageRefresh->setAutoDefault(false);
    usageActions->addWidget(usageRefresh);usageLayout->addWidget(m_usageWarning);usageLayout->addLayout(usageActions);
    m_usageLimit->setPopoverContent(usagePanel);m_composer->toolbar()->add(ComposerToolbar::Slot::UsageLimit,m_usageLimit);
    connect(usageRefresh,&QPushButton::clicked,this,[this]{refreshAccountUsage(true);});
```

In `applyTheme` replace

```cpp
    m_usageWarning->setStyleSheet(QString("QLabel{color:%1;background:%2;border-radius:6px;padding:8px 12px;}").arg(m_dark?"#ffabab":"#a42330",m_dark?"#35252b":"#fff0f1"));
```

with

```cpp
    m_usageWarning->setStyleSheet(QString("QLabel{color:%1;}").arg(m_dark?"#ffabab":"#a42330"));
```

In `renderAccountUsage` replace

```cpp
    m_usageStrip->setVisible(visible);m_usageWarning->setVisible(false);if(!visible){m_usageRefresh->setRefreshing(false);return;}
    m_accountUsageData=m_usageStore->data(AccountUsageRef::bound(entry->host,entry->session));m_accountUsage->setData(m_accountUsageData);
    const auto warning=AccountUsage::exhausted(m_accountUsageData);m_usageWarning->setText(warning);m_usageWarning->setVisible(!warning.isEmpty());
```

with

```cpp
    m_usageStrip->setVisible(visible);m_usageLimit->setActive(false);if(!visible){m_usageRefresh->setRefreshing(false);return;}
    m_accountUsageData=m_usageStore->data(AccountUsageRef::bound(entry->host,entry->session));m_accountUsage->setData(m_accountUsageData);
    const auto warning=AccountUsage::exhausted(m_accountUsageData);m_usageWarning->setText(warning);
    m_usageLimit->setLabels(AccountUsage::exhaustedSummary(m_accountUsageData),tr("Limit"));m_usageLimit->setDetail(warning);
    m_usageLimit->setActive(!warning.isEmpty());
```

- [ ] **Step 5: Run the tests**

Run: `cmake --build tray/build --target test_accountusage test_sessionswindow -j16 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_accountusage && QT_QPA_PLATFORM=offscreen tray/build/tests/test_sessionswindow usageLimitBecomesToolbarChip accountUsageRejectsOtherSessionReplies`
Expected: all pass.

Run: `ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: 10/10 pass.

- [ ] **Step 6: Commit**

```bash
git commit --only -m "Show an exhausted account limit as a toolbar chip" -- \
  tray/src/AccountUsage.h tray/src/SessionsWindow.h tray/src/SessionsWindow.cpp \
  tray/tests/test_accountusage.cpp tray/tests/test_sessionswindow.cpp
```

---

### Task 7: Recovery chip with the panel as its popover

**Files:**
- Modify: `tray/src/RecoveryWidgets.h` (`Panel`, lines ~38-115)
- Modify: `tray/src/SessionsWindow.h`, `tray/src/SessionsWindow.cpp` (~601-609 construction, theme ~1266, focus rule ~2326)
- Modify: `tray/tests/test_sessionswindow.cpp` (~909-935, ~1535-1545)

**Interfaces:**
- Consumes: `ToolbarChip`, `ChipTone` (Task 1); `Slot::Recovery`.
- Produces: `RecoveryUi::Panel::active()`, `struct RecoveryUi::Panel::Summary { QString label, shortLabel; ChipTone tone; }`, `Panel::summary()`, `Panel::detailText()`, `std::function<void()> Panel::summaryChanged`; `ToolbarChip *SessionsWindow::m_recoveryChip` (`recoveryChip`).

- [ ] **Step 1: Rewrite the recovery tests**

In the recovery countdown test (~909-935) replace

```cpp
    auto *panel=window.findChild<QFrame *>("recoveryPanel");QVERIFY(panel->isVisible());
```

with

```cpp
    auto *chip=window.findChild<ToolbarChip *>("recoveryChip");QVERIFY(chip && chip->isVisible());QCOMPARE(chip->tone(),ChipTone::Warning);
    QVERIFY(chip->fullLabel().startsWith("Retry in"));QVERIFY(chip->shortLabel().endsWith(" s"));
    auto *panel=window.findChild<QFrame *>("recoveryPanel");QVERIFY(!panel->isVisible());
```

replace `const int revision=browser->document()->revision();const auto geometry=browser->geometry();` with

```cpp
    const int revision=browser->document()->revision();const auto geometry=browser->geometry();const auto label=chip->fullLabel();
```

after `QCOMPARE(browser->document()->revision(),revision);QCOMPARE(browser->geometry(),geometry);` add

```cpp
    QVERIFY(chip->fullLabel()!=label);   // the countdown ticks while the popover is closed
```

before the `QTimer::singleShot(0,&window,[&]{ … history … });` block add

```cpp
    QTest::mouseClick(chip,Qt::LeftButton);QTRY_VERIFY(panel->isVisible());
    if(!preview.isEmpty())QVERIFY(chip->popover()->grab().save(preview+"/recovery-popover.png"));
```

after `window.activateWindow();QTest::qWait(20);` (following the history click) add

```cpp
    if(!panel->isVisible()){QTest::mouseClick(chip,Qt::LeftButton);QTRY_VERIFY(panel->isVisible());}
```

after `QVERIFY(panel->hasFocus());QVERIFY(!panel->findChild<QPushButton *>("recoveryNow")->isVisible());` add

```cpp
    QCOMPARE(chip->tone(),ChipTone::Warning);QCOMPARE(chip->shortLabel(),QString("Off"));
```

and replace the last line

```cpp
    QVERIFY(!panel->isVisible());QVERIFY(editor->hasFocus());QCOMPARE(editor->toPlainText(),QString("Keep my draft"));
```

with

```cpp
    QVERIFY(chip->isHidden());QVERIFY(!panel->isVisible());QTRY_VERIFY(editor->hasFocus());QCOMPARE(editor->toPlainText(),QString("Keep my draft"));
```

In the quota provider-error test (~1535) replace

```cpp
    auto *panel=window.findChild<QFrame *>("recoveryPanel");QVERIFY(panel && panel->isVisible());
```

with

```cpp
    auto *chip=window.findChild<ToolbarChip *>("recoveryChip");QVERIFY(chip && chip->isVisible());
    QCOMPARE(chip->fullLabel(),QString("Provider error"));QCOMPARE(chip->tone(),ChipTone::Danger);
    auto *panel=window.findChild<QFrame *>("recoveryPanel");QVERIFY(panel && !panel->isVisible());
    window.activateWindow();QTest::mouseClick(chip,Qt::LeftButton);QTRY_VERIFY(panel->isVisible());
```

replace `window.activateWindow();auto *action=panel->findChild<QPushButton *>("providerErrorTerminal");action->setFocus();` with

```cpp
    auto *action=panel->findChild<QPushButton *>("providerErrorTerminal");action->setFocus();
```

and after `QCOMPARE(composer->editor()->toPlainText(),QString("Continue after account access is restored"));` add

```cpp
    // Switching session closes the popover instead of showing this session's error elsewhere.
    window.showSession("mac","claude/infra/review");QVERIFY(!panel->isVisible());
```

(If the `HGS_QUOTA_PREVIEW` block below that line relies on the selected session, move this new `showSession` line after the preview block.)

- [ ] **Step 2: Run to verify they fail**

Run: `cmake --build tray/build --target test_sessionswindow -j16 && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: two functions FAIL with `chip` null (`recoveryChip` does not exist yet).

- [ ] **Step 3: Give the panel a summary**

In `tray/src/RecoveryWidgets.h`:
- add `#include "ComposerToolbar.h"` after `#include "SessionPresentation.h"`;
- in `Panel`, after `std::function<void()> refreshUsage;` add:

```cpp
    // The toolbar chip follows the panel: called on every refresh, also while hidden.
    std::function<void()> summaryChanged;
    struct Summary { QString label, shortLabel; ChipTone tone; };
```

- in the constructor, replace the timer line ending `…if(isVisible())refresh();});timer->start(1000);hide();` with:

```cpp
        auto *timer=new QTimer(this);connect(timer,&QTimer::timeout,this,[this]{if(m_active)refresh();});timer->start(1000);
```

- in `setState`, replace

```cpp
        setVisible((!job.isEmpty() && job.value("state")!="succeeded") || !m_failure.isEmpty()); refresh();
```

with

```cpp
        m_active=(!job.isEmpty() && job.value("state")!="succeeded") || !m_failure.isEmpty(); refresh();
```

- after `void finished(const QString &error) { … }` add:

```cpp
    bool active() const { return m_active; }
    QString detailText() const { return QStringList{m_status->text(),m_detail->text()}.join('\n').trimmed(); }
    Summary summary() const {
        if(isFailure())return {tr("Provider error"),tr("Error"),ChipTone::Danger};
        const auto state=m_job.value("state").toString();const auto label=RecoveryUi::status(m_job);
        if(state=="waiting"||state=="dispatching"||state=="retrying") {
            const int remaining=qMax(0,int(std::ceil(m_job.value("due_at").toDouble()-QDateTime::currentMSecsSinceEpoch()/1000.0)));
            return {label,state=="waiting"&&remaining>0?tr("%1 s").arg(remaining):QStringLiteral("…"),ChipTone::Warning};
        }
        if(state=="cancelled")return {label,tr("Off"),ChipTone::Warning};
        return {label,tr("Error"),ChipTone::Danger};
    }
```

- in `refresh()`, replace its first statement

```cpp
        const bool failure=!m_failure.isEmpty() && (m_job.isEmpty() || m_job.value("state")=="succeeded"
            || m_failure.value("error_kind")=="quota");
```

with `const bool failure=isFailure();`, and make every exit of `refresh()` notify the chip: change the `return;` at the end of the `if(failure) { … }` block to `notify();return;`, and add `notify();` as the last statement of `refresh()`.
- add these private helpers before `QJsonObject m_job,m_failure;`:

```cpp
    bool isFailure() const {
        return !m_failure.isEmpty() && (m_job.isEmpty() || m_job.value("state")=="succeeded" || m_failure.value("error_kind")=="quota");
    }
    void notify() { if(summaryChanged)summaryChanged(); }
```

- add `bool m_active=false;` to the members (`QJsonObject m_job,m_failure; bool m_online=false,m_pending=false,m_active=false; QString m_error;`).

- [ ] **Step 4: Put the panel behind the chip**

In `tray/src/SessionsWindow.h` add `ToolbarChip *m_recoveryChip = nullptr;` after `RecoveryUi::Panel *m_recovery = nullptr;`.

In `tray/src/SessionsWindow.cpp` replace

```cpp
    m_recovery = new RecoveryUi::Panel; activityLayout->addWidget(m_recovery);
    m_recovery->openTerminal = [this] {
        const auto *entry=selected();if(!entry || !entry->online)return;
```

with

```cpp
    m_recovery = new RecoveryUi::Panel;
    m_recoveryChip=new ToolbarChip;m_recoveryChip->setObjectName("recoveryChip");m_recoveryChip->setIconName("refresh");
    m_recoveryChip->setPopoverContent(m_recovery);m_composer->toolbar()->add(ComposerToolbar::Slot::Recovery,m_recoveryChip);
    m_recovery->summaryChanged = [this] {
        const auto summary=m_recovery->summary();
        m_recoveryChip->setLabels(summary.label,summary.shortLabel);m_recoveryChip->setTone(summary.tone);
        m_recoveryChip->setDetail(m_recovery->detailText());m_recoveryChip->setActive(m_recovery->active());
    };
    m_recovery->openTerminal = [this] {
        m_recoveryChip->closePopover();
        const auto *entry=selected();if(!entry || !entry->online)return;
```

and replace

```cpp
    m_recovery->openSettings = [this] {
        showWorkspaceSettings();m_settingsPage->openRecovery();
```

with

```cpp
    m_recovery->openSettings = [this] {
        m_recoveryChip->closePopover();showWorkspaceSettings();m_settingsPage->openRecovery();
```

In `applyTheme` replace

```cpp
    m_recovery->setStyleSheet(QString("QFrame#recoveryPanel {background:%1;border:1px solid %2;border-radius:8px;} QLabel {border:0;background:transparent;}").arg(m_surface,m_border));
```

with (the popover now draws the frame)

```cpp
    m_recovery->setStyleSheet(QStringLiteral("QFrame#recoveryPanel {background:transparent;border:0;} QLabel {border:0;background:transparent;}"));
```

The focus rule in `renderDetails` (`…&&m_recovery->isVisible()&&recoveryFocus…`) stays unchanged: `isVisible()` now means "popover open", which is exactly when the panel can hold focus.

- [ ] **Step 5: Run the window suite**

Run: `cmake --build tray/build --target test_sessionswindow -j16 && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: 10/10 pass.

- [ ] **Step 6: Commit**

```bash
git commit --only -m "Collapse the recovery panel into a ticking toolbar chip" -- \
  tray/src/RecoveryWidgets.h tray/src/SessionsWindow.h tray/src/SessionsWindow.cpp tray/tests/test_sessionswindow.cpp
```

---

### Task 8: Visual check and full verification

**Files:**
- Modify: `tray/tests/test_messagecomposer.cpp` (optional preview function)

**Interfaces:**
- Consumes: everything above.
- Produces: screenshots for review; a green full suite.

- [ ] **Step 1: Add the preview test**

Declare `void toolbarPreview();` in `TestMessageComposer` and add:

```cpp
void TestMessageComposer::toolbarPreview()
{
    const auto directory = qEnvironmentVariable("HGS_COMPOSER_PREVIEW");
    if (directory.isEmpty()) QSKIP("Set HGS_COMPOSER_PREVIEW to write screenshots");
    QVERIFY(QDir().mkpath(directory));
    for (const bool dark : {true, false}) {
        QWidget window; window.setAutoFillBackground(true);
        QPalette palette = window.palette(); palette.setColor(QPalette::Window, QColor(dark ? "#1b2129" : "#ffffff")); window.setPalette(palette);
        auto *layout = new QVBoxLayout(&window); layout->setContentsMargins(16, 12, 16, 12);
        auto *composer = new MessageComposer; layout->addWidget(composer);
        composer->setSessionKey(QString("local/preview-%1").arg(dark ? "dark" : "light")); composer->setAvailability(true);
        const auto chip = [&](ComposerToolbar::Slot slot, const char *name, const QString &full, const QString &shortText, const QString &icon, ChipTone tone) {
            auto *item = new ToolbarChip; item->setObjectName(name); item->setLabels(full, shortText); item->setIconName(icon); item->setTone(tone);
            composer->toolbar()->add(slot, item); return item;
        };
        auto *read = chip(ComposerToolbar::Slot::MarkRead, "read", "Mark as read", {}, "read-all", ChipTone::Quiet);
        auto *recovery = chip(ComposerToolbar::Slot::Recovery, "recovery", "Retry in 42 s", "42 s", "refresh", ChipTone::Warning);
        auto *cache = chip(ComposerToolbar::Slot::Cache, "cache", "Cache ~4m", "~4m", {}, ChipTone::Success);
        auto *limit = chip(ComposerToolbar::Slot::UsageLimit, "limit", "Limit reached · 2h 14m", "Limit", "attention", ChipTone::Danger);
        auto *context = new QPushButton("70,1k"); context->setFlat(true); context->setFixedHeight(24); composer->toolbar()->add(ComposerToolbar::Slot::Context, context);
        composer->setTheme(dark); cache->setActive(true);
        window.resize(760, 200); window.show(); QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto save = [&](const QString &name) {
            QTest::qWait(80); QVERIFY(window.grab().save(QString("%1/%2-%3.png").arg(directory, name, dark ? "dark" : "light")));
        };
        save("1-normal");
        read->setActive(true);
        for (const auto &name : {"shot.png", "screen.png"}) QVERIFY(composer->addAttachment(name, "image/png", pngBytes()));
        QVERIFY(composer->addAttachment("notes.md", "text/markdown", "# notes"));
        save("2-attached-unread");
        auto *attachments = composer->findChild<ToolbarChip *>("attachmentsChip"); attachments->openPopover(); QTest::qWait(80);
        QVERIFY(attachments->popover()->grab().save(QString("%1/3-attachments-popover-%2.png").arg(directory, dark ? "dark" : "light")));
        attachments->closePopover();
        cache->setLabels("Cold cache", "Cold"); cache->setTone(ChipTone::Danger); cache->setIconName("context-warning");
        recovery->setActive(true); save("5-recovery");
        recovery->setActive(false); limit->setActive(true); save("6-limit");
        recovery->setActive(true); window.resize(420, 200); save("7-narrow");
    }
}
```

- [ ] **Step 2: Produce the screenshots**

```bash
cmake --build tray/build --target test_messagecomposer test_sessionswindow -j16
P=/tmp/claude-1000/-home-n-prudnikov-w-zerus/composer-preview; mkdir -p "$P"
HGS_COMPOSER_PREVIEW=$P QT_QPA_PLATFORM=offscreen tray/build/tests/test_messagecomposer toolbarPreview
HGS_RECOVERY_PREVIEW=$P/recovery HGS_QUOTA_PREVIEW=$P/quota HGS_COMPACT_PREVIEW=$P/compact.png \
HGS_WORKTREE_PREVIEW=$P/worktree HGS_QUESTION_FOOTER_PREVIEW=$P/question HGS_ATTENTION_PREVIEW=$P/attention \
  QT_QPA_PLATFORM=offscreen tray/build/tests/test_sessionswindow
```

Expected: PNGs in `$P`. Open each with the Read tool and compare with the reviewed mockups (`.superpowers/brainstorm/*/content/toolbar-states.html`, states 1–7): one row of chips above the field, warning/danger chips tinted, context counter rightmost, nothing overlapping at 420 px, popovers above their chips, light and dark both legible. Fix any visual defect in the owning task's file before continuing.

- [ ] **Step 3: Run the complete Qt suite in a separate build directory**

```bash
cmake -S tray -B .ci-build/gui -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON \
  -DCMAKE_PREFIX_PATH="$PWD/.deps/Qt/6.11.3/gcc_64;$PWD/.deps/kf6"
ZERUS_BUILD_JOBS=16 bash scripts/ci/gui.sh
python3 scripts/ci/check-source.py
```

Expected: `100% tests passed` in `artifacts/test-results/qt.log`; `Source syntax, privacy and release version checks passed.`

- [ ] **Step 4: Commit and close the issue**

```bash
git commit --only -m "Add an optional screenshot preview of composer toolbar states" -- tray/tests/test_messagecomposer.cpp
bd close zerus-sdr --reason="Composer toolbar replaces the footer, notices and attachment strip"
bd export -o .beads/issues.jsonl
```

If `bd export` refuses because `.beads/issues.jsonl` holds records missing from the Dolt store, do not import or overwrite anything: report it and leave `.beads/` unstaged. Report `git status --short` and the commit list (`git log --oneline -9`) at handoff; do not push.
