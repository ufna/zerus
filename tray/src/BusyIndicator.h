#pragma once
#include <QHideEvent>
#include <QPainter>
#include <QShowEvent>
#include <QTimer>
#include <QWidget>

// A small, non-interactive loading arc. Hidden pages do not keep animating.
class BusyIndicator : public QWidget {
public:
    explicit BusyIndicator(QWidget *parent = nullptr) : QWidget(parent) {
        setFixedSize(16,16);
        auto policy=sizePolicy(); policy.setRetainSizeWhenHidden(true); setSizePolicy(policy);
        setAttribute(Qt::WA_TransparentForMouseEvents);
        setAccessibleName(tr("Loading"));
        m_timer.setInterval(40);
        connect(&m_timer,&QTimer::timeout,this,[this] { m_angle=(m_angle+18)%360; update(); });
        hide();
    }
    void setTheme(bool dark) { m_color=QColor(dark?"#e9b949":"#9b6b00"); update(); }
    void setRunning(bool running) {
        if (m_running==running) return;
        m_running=running; m_angle=0;
        setVisible(running);
        if (running && isVisible()) m_timer.start(); else m_timer.stop();
        update();
    }
protected:
    void showEvent(QShowEvent *event) override {
        QWidget::showEvent(event);
        if (m_running) m_timer.start();
    }
    void hideEvent(QHideEvent *event) override { m_timer.stop(); QWidget::hideEvent(event); }
    void paintEvent(QPaintEvent *) override {
        if (!m_running) return;
        QPainter p(this); p.setRenderHint(QPainter::Antialiasing);
        p.setPen(QPen(m_color,2.2,Qt::SolidLine,Qt::RoundCap));
        p.drawArc(QRectF(3,3,10,10),(90-m_angle)*16,260*16);
    }
private:
    QTimer m_timer;
    QColor m_color{"#e9b949"};
    int m_angle=0;
    bool m_running=false;
};
