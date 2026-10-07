#pragma once

#include <QObject>
#include <QProcessEnvironment>
#include <QSize>
#include <QTimer>

class QSocketNotifier;

// Owns only the attach client and its private controlling terminal. The tmux
// server, pane supervisor and agent remain outside this process group.
class PtyProcess final : public QObject {
  Q_OBJECT
public:
  explicit PtyProcess(QObject *parent = nullptr);
  ~PtyProcess() override;
  bool start(const QString &program, const QStringList &arguments,
             const QProcessEnvironment &environment, QSize cells);
  void stop();
  void write(const QByteArray &bytes);
  void resize(QSize cells);
  bool isRunning() const { return m_pid > 0; }
  qint64 processId() const { return m_pid; }

signals:
  void output(const QByteArray &bytes);
  void finished(int exitCode);
  void errorOccurred(const QString &message);

private:
  void readReady();
  void writeReady();
  void reap();
  void closeMaster();
  int m_master = -1;
  int m_pid = -1;
  QByteArray m_pending;
  QSocketNotifier *m_reader = nullptr;
  QSocketNotifier *m_writer = nullptr;
  QTimer m_reaper;
};
