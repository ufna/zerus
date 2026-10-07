#include "NativeUiView.h"
#include "HgsClient.h"
#include "WorkspaceIcons.h"
#include <QDesktopServices>
#include <QCryptographicHash>
#include <QFileDialog>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QPushButton>
#include <QRegularExpression>
#include <QStackedWidget>
#include <QStandardPaths>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>
#include <QWebEngineDownloadRequest>
#include <QWebEngineNewWindowRequest>
#include <QWebEnginePage>
#include <QWebEngineProfile>
#include <QWebEngineSettings>
#include <QWebEngineView>

namespace {
QString originOf(const QUrl &url) { return url.adjusted(QUrl::RemovePath | QUrl::RemoveQuery | QUrl::RemoveFragment).toString(); }
bool webLink(const QUrl &url) { return url.scheme() == "https" || url.scheme() == "http"; }
class NativePage final : public QWebEnginePage {
public:
    NativePage(QWebEngineProfile *profile, const QString &origin, QObject *parent)
        : QWebEnginePage(profile, parent), m_origin(origin) {
        connect(this, &QWebEnginePage::newWindowRequested, this, [this](QWebEngineNewWindowRequest &request) {
            const auto url = request.requestedUrl();
            if (originOf(url) == m_origin) request.openIn(this);
            else if (request.isUserInitiated() && webLink(url)) QDesktopServices::openUrl(url);
        });
    }
protected:
    bool acceptNavigationRequest(const QUrl &url, NavigationType type, bool mainFrame) override {
        if (!mainFrame || url == QUrl("about:blank") || originOf(url) == m_origin) return true;
        if (type == NavigationTypeLinkClicked && webLink(url)) QDesktopServices::openUrl(url);
        return false;
    }
    // Native JS diagnostics can contain an authenticated address. Keep them
    // out of Zerus logs; visible load failures never include the launch URL.
    void javaScriptConsoleMessage(JavaScriptConsoleMessageLevel, const QString &, int, const QString &) override {}
private:
    QString m_origin;
};
}

NativeUiView::NativeUiView(HgsClient *client, QWidget *parent) : QWidget(parent), m_client(client)
{
    setObjectName("nativeUiView");
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 8, 0, 0); layout->setSpacing(8);
    auto *tools = new QHBoxLayout;
    m_title = new QLabel(tr("DeepSeek native workspace")); m_title->setTextFormat(Qt::PlainText);
    tools->addWidget(m_title, 1);
    m_response = new QPushButton(tr("Respond in Activity")); m_response->setObjectName("nativeUiResponse");
    m_response->setToolTip(tr("This session has a question or approval waiting in Activity."));
    m_response->hide(); tools->addWidget(m_response);
    connect(m_response, &QPushButton::clicked, this, &NativeUiView::responseRequested);
    m_reload = new QPushButton; m_reload->setObjectName("reloadNativeUi");
    m_reload->setIcon(workspaceIcon("refresh", palette().color(QPalette::Text)));
    m_reload->setToolTip(tr("Reload native UI")); m_reload->setAccessibleName(tr("Reload native UI"));
    m_reload->setFixedSize(32, 32); tools->addWidget(m_reload); layout->addLayout(tools);
    m_status = new QLabel; m_status->setTextFormat(Qt::PlainText); m_status->setWordWrap(true);
    m_status->setObjectName("nativeUiStatus"); layout->addWidget(m_status);
    m_stack = new QStackedWidget; layout->addWidget(m_stack, 1);
    connect(m_reload, &QPushButton::clicked, this, [this] { connectUi(true); });
    connect(client, &HgsClient::nativeUiFailed, this, [this](quint64 request, const QString &error) {
        if (request != m_request) return;
        m_request = 0; m_reload->setEnabled(m_available); showProblem(tr("Could not open native DeepSeek UI. %1").arg(error));
    });
    connect(client, &HgsClient::nativeUiReady, this, [this](quint64 request, const QJsonObject &connection) {
        if (request != m_request || !m_available) return;
        m_request = 0; m_reload->setEnabled(true);
        const QString address = connection.value("url").toString();
        const QUrl url(address);
        static const QRegularExpression launch("^http://127\\.0\\.0\\.1:[0-9]+/\\?token=[A-Za-z0-9_%.-]+$");
        if (!launch.match(address).hasMatch() || url.port() <= 0 || url.port() > 65535) {
            showProblem(tr("The native UI returned an invalid connection address.")); return;
        }
        const QString origin = originOf(url);
        // Reused loopback ports must never revive another host generation's
        // page or cookies. Keep only a fingerprint of its launch credential.
        const QString key = m_host + '\n' + origin + '\n' + QString::fromLatin1(QCryptographicHash::hash(address.toUtf8(), QCryptographicHash::Sha256).toHex());
        QWebEngineView *view = nullptr;
        for (const auto &browser : m_browsers) if (browser.key == key) view = browser.view;
        const bool fresh = !view;
        if (!view) {
            auto *profile = new QWebEngineProfile(this); // Off the record, including cookies and cache.
            view = new QWebEngineView; view->setObjectName("nativeWebPage");
            auto *page = new NativePage(profile, origin, view); view->setPage(page);
            page->setBackgroundColor(palette().color(QPalette::Window));
            page->settings()->setAttribute(QWebEngineSettings::LocalContentCanAccessFileUrls, false);
            m_stack->addWidget(view); m_browsers.append({key, profile, view});
            connect(view, &QWebEngineView::loadFinished, this, [this, view, key](bool ok) {
                if (m_stack->currentWidget() != view || m_activeOrigin != key || !m_available) return;
                if (ok) { m_status->hide(); m_stack->show(); m_connected = true; }
                else showProblem(tr("Native UI could not load. Use Reload to reconnect, or open it in your browser."));
            });
            connect(view, &QWebEngineView::renderProcessTerminated, this, [this, view, key] {
                if (m_stack->currentWidget() == view && m_activeOrigin == key) showProblem(tr("The native UI renderer stopped. Use Reload to reopen it."));
            });
            connect(profile, &QWebEngineProfile::downloadRequested, this, [this](QWebEngineDownloadRequest *download) {
                const auto path = QFileDialog::getSaveFileName(this, tr("Save file"), QStandardPaths::writableLocation(QStandardPaths::DownloadLocation) + '/' + QFileInfo(download->suggestedFileName()).fileName());
                if (path.isEmpty()) { download->cancel(); return; }
                download->setDownloadDirectory(QFileInfo(path).absolutePath()); download->setDownloadFileName(QFileInfo(path).fileName()); download->accept();
            });
        }
        m_activeOrigin = key; m_stack->setCurrentWidget(view); m_stack->show(); m_connected = true;
        if (fresh || m_forceReload) {
            m_status->setText(tr("Loading native DeepSeek UI…")); m_status->show(); view->load(url);
        } else m_status->hide();
    });
}

NativeUiView::~NativeUiView()
{
    // A browser profile must outlive every page using it.
    for (const auto &browser : m_browsers) { delete browser.view; delete browser.profile; }
}

void NativeUiView::setSession(const QString &host, const QString &name, const QString &run, const QString &machine, bool available)
{
    const QString identity = host + '\n' + name + '\n' + run;
    if (identity != m_identity || available != m_available) {
        m_request = 0; m_connected = false; m_identity = identity; m_activeOrigin.clear(); m_stack->hide();
        m_status->setText(available ? tr("Open the native workspace to manage DeepSeek sessions and settings.") : tr("Native UI is unavailable while this machine is offline.")); m_status->show();
        if (available && isVisible()) QTimer::singleShot(0, this, &NativeUiView::activate);
    }
    m_host = host; m_name = name; m_available = available && !name.isEmpty();
    m_title->setText(tr("DeepSeek workspace / %1").arg(machine)); m_reload->setEnabled(m_available && !m_request);
}

void NativeUiView::activate() { if (!m_connected && !m_request) connectUi(false); }
void NativeUiView::setResponsePending(bool pending) { m_response->setVisible(pending); }
void NativeUiView::showEvent(QShowEvent *event) { QWidget::showEvent(event); activate(); }
void NativeUiView::connectUi(bool reload)
{
    if (!m_available || m_request) return;
    m_forceReload = reload; m_connected = false; m_reload->setEnabled(false);
    m_status->setText(tr("Connecting to native DeepSeek UI…")); m_status->show();
    m_request = m_client->requestNativeUi(m_host, m_name);
}
void NativeUiView::showProblem(const QString &text)
{
    m_connected = false; m_status->setText(text); m_status->show(); m_stack->hide();
}
