#pragma once
#include <QElapsedTimer>
#include <QEvent>
#include <QPainter>
#include <QPushButton>
#include <QScreen>
#include <QTimer>
#include <QtMath>
#include <cmath>

// Keep the button's native hover/focus/checked painting. Only its cached mark
// rotates: replacing QIcon on every frame also invalidates button layout.
class SwarmButton : public QPushButton {
public:
    explicit SwarmButton(QWidget *parent = nullptr) : QPushButton(parent) {
        m_timer.setTimerType(Qt::PreciseTimer); m_timer.setSingleShot(true);
        connect(&m_timer, &QTimer::timeout, this, [this] { update(); scheduleFrame(); });
    }
    void setAppearance(const QColor &neutral, const QColor &attention, bool active, bool reduced) {
        m_neutral = neutral; m_attentionColor = attention;
        m_attention = active; m_reduced = reduced;
        syncAnimation();
    }
    void syncAnimation() {
        if (m_attention && !m_reduced && isVisible() && !window()->isMinimized()) {
            if (!m_clock.isValid()) m_clock.start();
            if (!m_timer.isActive()) scheduleFrame();
        } else {
            m_timer.stop(); m_clock.invalidate();
        }
        update();
    }
protected:
    void showEvent(QShowEvent *event) override { QPushButton::showEvent(event); syncAnimation(); }
    void hideEvent(QHideEvent *event) override { QPushButton::hideEvent(event); syncAnimation(); }
    void paintEvent(QPaintEvent *event) override {
        QPushButton::paintEvent(event);
        const QColor color = m_attention ? m_attentionColor : m_neutral;
        const qreal ratio = devicePixelRatioF();
        if (m_mark.isNull() || color != m_markColor || ratio != m_mark.devicePixelRatio()) {
            m_mark = QPixmap(":/hgs/icons/hgs-zerus-128.png").scaled(qCeil(28 * ratio), qCeil(28 * ratio),
                Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
            QPainter tint(&m_mark); tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
            tint.fillRect(m_mark.rect(), color); tint.end();
            m_mark.setDevicePixelRatio(ratio); m_markColor = color;
        }
        const qreal progress = m_clock.isValid() ? qMin<qreal>(1, (m_clock.elapsed() % 3000) / 1400.0) : 0;
        QPainter painter(this); painter.setRenderHint(QPainter::SmoothPixmapTransform);
        painter.translate(width() / 2.0, height() / 2.0);
        if (m_clock.isValid()) {
            painter.rotate(180 * (1 - std::cos(M_PI * progress)));
            painter.setOpacity(.65 + .35 * std::sin(M_PI * progress));
        }
        painter.drawPixmap(QPointF(-14, -14), m_mark);
    }
private:
    void scheduleFrame() {
        const auto phase = m_clock.elapsed() % 3000;
        // Nothing moves during the pause; wake once for the next revolution.
        const qreal hz = screen() ? qBound<qreal>(60, screen()->refreshRate(), 120) : 60;
        m_timer.start(phase >= 1400 ? int(3000 - phase) : qMax(8, int(1000 / hz)));
    }
    QTimer m_timer;
    QElapsedTimer m_clock;
    QPixmap m_mark;
    QColor m_markColor, m_neutral = Qt::white, m_attentionColor = QColor("#ffda76");
    bool m_attention = false, m_reduced = false;
};
