#pragma once
#include <QAbstractButton>
#include <QApplication>
#include <QFocusEvent>
#include <QKeyEvent>
#include <QStyle>

// QSS has :focus but no :focus-visible. Keep keyboard accessibility while
// preventing clicks and focus restoration after dialogs from leaving a ring.
class WorkspaceFocus : public QObject {
public:
    static void install() {
        if (qApp && !qApp->findChild<QObject *>("workspaceFocus", Qt::FindDirectChildrenOnly)) new WorkspaceFocus;
    }
protected:
    bool eventFilter(QObject *object, QEvent *event) override {
        auto *button = qobject_cast<QAbstractButton *>(object);
        if (event->type() == QEvent::MouseButtonPress || event->type() == QEvent::TouchBegin) {
            m_keyboard = false;
            indicate(qobject_cast<QAbstractButton *>(QApplication::focusWidget()), false);
            indicate(button, false);
        } else if (event->type() == QEvent::KeyPress) {
            const auto key = static_cast<QKeyEvent *>(event)->key();
            if (key != Qt::Key_Shift && key != Qt::Key_Control && key != Qt::Key_Alt && key != Qt::Key_Meta) {
                m_keyboard = true;
                indicate(qobject_cast<QAbstractButton *>(QApplication::focusWidget()), true);
                indicate(button, true);
            }
        } else if (button && event->type() == QEvent::FocusIn) {
            const auto reason = static_cast<QFocusEvent *>(event)->reason();
            // Qt also uses TabFocusReason when a clicked button disables
            // itself. Only actual key input (above) or a shortcut establishes
            // keyboard modality; automatic focus transfers preserve it.
            if (reason == Qt::ShortcutFocusReason) m_keyboard = true;
            else if (reason == Qt::MouseFocusReason) m_keyboard = false;
            indicate(button, m_keyboard);
        }
        return false;
    }
private:
    WorkspaceFocus() : QObject(qApp) { setObjectName("workspaceFocus"); qApp->installEventFilter(this); }
    static void indicate(QAbstractButton *button, bool keyboard) {
        if (!button || (button->property("keyboardFocus").isValid() && button->property("keyboardFocus").toBool() == keyboard)) return;
        button->setProperty("keyboardFocus", keyboard);
        button->setAttribute(Qt::WA_MacShowFocusRect, keyboard);
        button->style()->unpolish(button); button->style()->polish(button); button->update();
    }
    bool m_keyboard = false;
};
