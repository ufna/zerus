#pragma once
#include <QFocusEvent>
#include <QKeyEvent>
#include <QListWidget>
#include <QMouseEvent>
#include <QStyleOptionViewItem>

// Keep real keyboard focus and selection; only its visual indicator follows
// input modality. Qt's window flag alone misses arrows after a mouse click.
class WorkspaceList : public QListWidget {
public:
    using QListWidget::QListWidget;
protected:
    void setKeyboardFocusVisible(bool visible) {
        if (m_keyboardFocus == visible) return;
        m_keyboardFocus = visible; viewport()->update();
    }
    void initViewItemOption(QStyleOptionViewItem *option) const override {
        QListWidget::initViewItemOption(option);
        option->state.setFlag(QStyle::State_KeyboardFocusChange, m_keyboardFocus);
    }
    void focusInEvent(QFocusEvent *event) override {
        if (event->reason() == Qt::TabFocusReason || event->reason() == Qt::BacktabFocusReason || event->reason() == Qt::ShortcutFocusReason) setKeyboardFocusVisible(true);
        else if (event->reason() == Qt::MouseFocusReason) setKeyboardFocusVisible(false);
        QListWidget::focusInEvent(event);
    }
    void mousePressEvent(QMouseEvent *event) override { setKeyboardFocusVisible(false); QListWidget::mousePressEvent(event); }
    void keyPressEvent(QKeyEvent *event) override { setKeyboardFocusVisible(true); QListWidget::keyPressEvent(event); }
private:
    bool m_keyboardFocus = false;
};
