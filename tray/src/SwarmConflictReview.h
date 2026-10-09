#pragma once

#include <QJsonObject>
#include <QWidget>

class FleetState;
class QLabel;
class QPushButton;
class QTableWidget;

// Presents exactly the causal versions being reviewed. A refresh that changes
// those versions clears the choice; resolution never adopts unseen changes.
class SwarmConflictReview : public QWidget {
    Q_OBJECT
public:
    explicit SwarmConflictReview(const FleetState &fleet, QWidget *parent = nullptr);
    QString conflictLabel(const QJsonObject &conflict, const QJsonObject &snapshot) const;
    void setConflict(const QJsonObject &conflict, const QJsonObject &snapshot);
    void setBusy(bool busy);
signals:
    void resolutionRequested(const QJsonObject &payload);
private:
    QString projectName(const QString &id, const QJsonObject &snapshot) const;
    QString machineName(const QString &id, const QJsonObject &snapshot, bool origin = false) const;
    QString subject(const QJsonObject &field, const QJsonObject &snapshot, const QJsonObject &conflict = {}) const;
    QString valueText(const QJsonObject &field, const QJsonValue &value) const;
    QString consequence(const QJsonValue &value) const;
    void updateChoice();
    const FleetState &m_fleet;
    QJsonObject m_conflict, m_snapshot;
    QLabel *m_title, *m_subject, *m_explanation, *m_outcome;
    QTableWidget *m_choices;
    QPushButton *m_confirm;
    bool m_busy = false;
};
