#include "PtyProcess.h"

#include <QFile>
#include <QFileInfo>
#include <QSocketNotifier>
#include <QStandardPaths>
#include <algorithm>
#include <cerrno>
#include <csignal>
#include <cstring>
#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <thread>
#include <unistd.h>
#include <vector>
#ifdef Q_OS_MACOS
#include <util.h>
#else
#include <pty.h>
#endif

namespace {
constexpr qsizetype MaxQueuedInput = 4 * 1024 * 1024;
void reapLater(int pid) {
  std::thread([pid] {
    int status = 0;
    while (::waitpid(pid, &status, 0) < 0 && errno == EINTR) {
    }
  }).detach();
}
} // namespace

PtyProcess::PtyProcess(QObject *parent) : QObject(parent) {
  m_reaper.setInterval(75);
  connect(&m_reaper, &QTimer::timeout, this, &PtyProcess::reap);
}
PtyProcess::~PtyProcess() { stop(); }

bool PtyProcess::start(const QString &program, const QStringList &arguments,
                       const QProcessEnvironment &environment, QSize cells) {
  stop();
  const QString executable = QFileInfo(program).isAbsolute()
                                 ? program
                                 : QStandardPaths::findExecutable(program);
  if (executable.isEmpty() || !QFileInfo(executable).isExecutable() ||
      !QFileInfo(executable).isFile()) {
    emit errorOccurred(tr("Attach executable is unavailable: %1").arg(program));
    return false;
  }
  QList<QByteArray> argvStorage{QFile::encodeName(executable)};
  for (const QString &arg : arguments)
    argvStorage.append(arg.toUtf8());
  QList<QByteArray> envStorage;
  for (const QString &entry : environment.toStringList())
    envStorage.append(entry.toUtf8());
  std::vector<char *> argv, envp;
  for (QByteArray &arg : argvStorage)
    argv.push_back(arg.data());
  for (QByteArray &entry : envStorage)
    envp.push_back(entry.data());
  argv.push_back(nullptr);
  envp.push_back(nullptr);
  const int maxFd =
      static_cast<int>(std::clamp<long>(::sysconf(_SC_OPEN_MAX), 256, 65536));
  struct winsize size{};
  size.ws_col = static_cast<unsigned short>(std::clamp(cells.width(), 2, 1000));
  size.ws_row = static_cast<unsigned short>(std::clamp(cells.height(), 2, 500));
  // A very fast Disconnect can arrive before the child resets Qt's signal
  // handlers. Keep termination blocked across fork until that reset is done.
  sigset_t blocked, previous;
  sigemptyset(&blocked);
  sigaddset(&blocked, SIGHUP);
  sigaddset(&blocked, SIGTERM);
  sigaddset(&blocked, SIGINT);
  sigaddset(&blocked, SIGQUIT);
  ::sigprocmask(SIG_BLOCK, &blocked, &previous);
  const int pid = ::forkpty(&m_master, nullptr, nullptr, &size);
  const int forkError = errno;
  if (pid != 0)
    ::sigprocmask(SIG_SETMASK, &previous, nullptr);
  if (pid < 0) {
    m_master = -1;
    emit errorOccurred(
        tr("Cannot open terminal: %1")
            .arg(QString::fromLocal8Bit(std::strerror(forkError))));
    return false;
  }
  if (pid == 0) {
    // Only async-signal-safe operations after fork in this Qt process.
    const int childSignals[] = {SIGINT,  SIGQUIT, SIGTERM, SIGHUP, SIGPIPE,
                                SIGCHLD, SIGTSTP, SIGTTIN, SIGTTOU};
    for (int signal : childSignals)
      ::signal(signal, SIG_DFL);
    sigset_t mask;
    sigemptyset(&mask);
    ::sigprocmask(SIG_SETMASK, &mask, nullptr);
    for (int fd = 3; fd < maxFd; ++fd)
      ::close(fd);
    ::execve(argv[0], argv.data(), envp.data());
    constexpr char message[] = "Unable to start the session attach client.\r\n";
    const auto ignored = ::write(STDERR_FILENO, message, sizeof(message) - 1);
    (void)ignored;
    ::_exit(127);
  }
  m_pid = pid;
  ::fcntl(m_master, F_SETFD, FD_CLOEXEC);
  ::fcntl(m_master, F_SETFL, ::fcntl(m_master, F_GETFL) | O_NONBLOCK);
  m_reader = new QSocketNotifier(m_master, QSocketNotifier::Read, this);
  m_writer = new QSocketNotifier(m_master, QSocketNotifier::Write, this);
  m_writer->setEnabled(false);
  connect(m_reader, &QSocketNotifier::activated, this, [this] { readReady(); });
  connect(m_writer, &QSocketNotifier::activated, this,
          [this] { writeReady(); });
  m_reaper.start();
  return true;
}

void PtyProcess::stop() {
  m_reaper.stop();
  const int pid = m_pid;
  m_pid = -1;
  // This group was created by forkpty. It contains only hgs a / ssh / the
  // tmux client; closing its PTY never targets any tmux server or agent.
  if (pid > 0)
    ::kill(-pid, SIGHUP);
  closeMaster();
  m_pending.clear();
  if (pid > 0) {
    int status = 0;
    const int waited = ::waitpid(pid, &status, WNOHANG);
    if (waited == 0 || (waited < 0 && errno == EINTR))
      reapLater(pid);
  }
}

void PtyProcess::closeMaster() {
  if (m_reader) {
    m_reader->setEnabled(false);
    m_reader->deleteLater();
    m_reader = nullptr;
  }
  if (m_writer) {
    m_writer->setEnabled(false);
    m_writer->deleteLater();
    m_writer = nullptr;
  }
  if (m_master >= 0) {
    ::close(m_master);
    m_master = -1;
  }
}
void PtyProcess::readReady() {
  QByteArray bytes;
  char buffer[16384];
  // Bound work per event-loop iteration; output-heavy agents must not freeze
  // the rest of the manager or its Disconnect action.
  while (m_master >= 0 && bytes.size() < 256 * 1024) {
    const auto n = ::read(m_master, buffer, sizeof(buffer));
    if (n > 0)
      bytes.append(buffer, n);
    else if (n < 0 && errno == EINTR)
      continue;
    else if (n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK))
      break;
    else {
      closeMaster();
      break;
    }
  }
  if (!bytes.isEmpty())
    emit output(bytes);
}
void PtyProcess::write(const QByteArray &bytes) {
  if (m_master < 0 || bytes.isEmpty())
    return;
  if (bytes.size() > MaxQueuedInput - m_pending.size()) {
    emit errorOccurred(
        tr("Terminal input queue is full; wait before pasting more text."));
    return;
  }
  m_pending.append(bytes);
  writeReady();
}
void PtyProcess::writeReady() {
  while (m_master >= 0 && !m_pending.isEmpty()) {
    const auto n = ::write(m_master, m_pending.constData(), m_pending.size());
    if (n > 0)
      m_pending.remove(0, n);
    else if (n < 0 && errno == EINTR)
      continue;
    else if (n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK))
      break;
    else {
      m_pending.clear();
      break;
    }
  }
  if (m_writer)
    m_writer->setEnabled(!m_pending.isEmpty());
}
void PtyProcess::resize(QSize cells) {
  if (m_master < 0)
    return;
  struct winsize size{};
  size.ws_col = static_cast<unsigned short>(std::clamp(cells.width(), 2, 1000));
  size.ws_row = static_cast<unsigned short>(std::clamp(cells.height(), 2, 500));
  ::ioctl(m_master, TIOCSWINSZ, &size);
}
void PtyProcess::reap() {
  if (m_pid <= 0)
    return;
  int status = 0;
  const int result = ::waitpid(m_pid, &status, WNOHANG);
  if (result == 0 || (result < 0 && errno == EINTR))
    return;
  readReady();
  m_pid = -1;
  m_reaper.stop();
  closeMaster();
  m_pending.clear();
  emit finished(result < 0          ? -1
                : WIFEXITED(status) ? WEXITSTATUS(status)
                                    : 128 + WTERMSIG(status));
}
