#include "FleetState.h"

#include <QStringList>
#include <QJsonDocument>
#include <QJsonArray>
#include <algorithm>

namespace {

int visibleSessionCount(const BoxState &box)
{
    return int(std::count_if(box.sessions.cbegin(), box.sessions.cend(),
                            [](const SessionInfo &session) { return session.state != QLatin1String("archived"); }));
}

// "just now" / "5m ago" — готовая фраза, без разбивки на число и единицу, потому что
// "just now" не про число.
//
// Минуты, а не секунды, и "just now" на полторы минуты. Возраст отвечает на вопрос "ноут
// спит или потеряли контакт" (см. setPeer), секунды ему не нужны -- а каждая смена текста
// это новый тултип у иконки, на маке -- обращение к NSStatusItem, которых должно быть как
// можно меньше (см. QuietTrayIcon.h). С секундами текст менялся почти на каждом
// 2-секундном тике: пир опрашивается раз в минуту, возраст всегда моложе минуты.
// Полторы минуты покрывают штатный минутный опрос вместе с задержкой ssh, так что
// здоровый пир не мигает "1m ago" перед каждым ответом; пропавший честно считает минуты.
QString formatAge(qint64 deltaMs)
{
    if (deltaMs < 0)
        deltaMs = 0;
    const qint64 sec = deltaMs / 1000;
    if (sec < 90)
        return QStringLiteral("just now");
    return QStringLiteral("%1m ago").arg(sec / 60);
}

} // namespace

namespace {
QString replyKey(const QString &host, const SessionInfo &s) {
    return QString::fromUtf8(QJsonDocument(QJsonArray{host, s.cmd, s.conversationId}).toJson(QJsonDocument::Compact));
}
QString attentionFallbackKey(const QString &host, const SessionInfo &s) {
    return QString::fromUtf8(QJsonDocument(QJsonArray{host, s.cmd, s.name, s.created}).toJson(QJsonDocument::Compact));
}
QString attentionKey(const QString &host, const SessionInfo &s) {
    return s.conversationId.isEmpty() ? attentionFallbackKey(host, s) : replyKey(host, s);
}
QString attentionFingerprint(const SessionInfo &s) {
    QJsonObject children;
    for (auto it = s.subagents.begin(); s.subagentSource != "hook_profiles" && it != s.subagents.end(); ++it) {
        const auto child = it.value().toObject();
        const auto phase = child.value("display_state").toString(child.value("state").toString());
        if (phase != "approval" && phase != "input" && phase != "attention") continue;
        const auto request = child.value("attention_id").toString(child.value("question_id").toString());
        children[it.key()] = QJsonArray{phase, request,
            request.isEmpty() ? child.value("last_event_at") : QJsonValue()};
    }
    // A native request stays the same across transcript/status updates. Keep a
    // timestamp fallback only for providers without a concrete request identity.
    return QString::fromUtf8(QJsonDocument(QJsonArray{s.runId,
        s.needsAction() ? s.phase : QString(), s.needsAction() ? s.attentionId : QString(),
        s.needsAction() && s.attentionId.isEmpty() ? QJsonValue(s.lastEventAt) : QJsonValue(), children})
        .toJson(QJsonDocument::Compact));
}
}
void FleetState::updateUnread(const QString &host, BoxState &box)
{
    for (auto &s : box.sessions) {
        s.unreadReply = s.state != "archived" && !s.conversationId.isEmpty() && !s.replyId.isEmpty()
            && m_readReplies.value(replyKey(host, s)).toString() != s.replyId;
        const auto key = attentionKey(host, s), fallback = attentionFallbackKey(host, s);
        if (key != fallback && m_attentionMarks.contains(fallback)) {
            if (!m_attentionMarks.contains(key)) m_attentionMarks[key] = m_attentionMarks.value(fallback);
            m_attentionMarks.remove(fallback);
        }
        const auto mark = m_attentionMarks.value(key).toObject();
        s.reviewLater = s.state != "archived" && mark.value("review").toBool();
        s.attentionAcknowledged = mark.value("acknowledged").toString() == attentionFingerprint(s);
    }
}
void FleetState::setAttentionMarks(const QJsonObject &marks)
{
    m_attentionMarks = marks; updateUnread({}, m_local);
    for (auto it = m_peers.begin(); it != m_peers.end(); ++it) updateUnread(it.key(), it->box);
}
SessionInfo *FleetState::matchingSession(const QString &host, const SessionInfo &expected)
{
    BoxState *box = host.isEmpty() ? &m_local : m_peers.contains(host) ? &m_peers[host].box : nullptr;
    if (!box) return nullptr;
    for (auto &s : box->sessions)
        if (s.name == expected.name && s.archiveId == expected.archiveId && s.conversationId == expected.conversationId
            && s.created == expected.created && s.state != "archived") return &s;
    return nullptr;
}
bool FleetState::setReviewLater(const QString &host, const SessionInfo &expected, bool marked)
{
    auto *s = matchingSession(host, expected); if (!s) return false;
    const auto key = attentionKey(host, *s);
    auto mark = m_attentionMarks.value(key).toObject(); mark["review"] = marked;
    m_attentionMarks[key] = mark; s->reviewLater = marked; return true;
}
bool FleetState::markSessionRead(const QString &host, const SessionInfo &expected)
{
    auto *s = matchingSession(host, expected);
    if (!s || s->replyId != expected.replyId || attentionFingerprint(*s) != attentionFingerprint(expected)) return false;
    markReplyRead(host, s->name, s->conversationId, s->replyId);
    const auto key = attentionKey(host, *s);
    auto mark = m_attentionMarks.value(key).toObject();
    mark["review"] = false; mark["acknowledged"] = attentionFingerprint(*s);
    m_attentionMarks[key] = mark; s->reviewLater = false; s->attentionAcknowledged = true; return true;
}
void FleetState::setReadReplies(const QJsonObject &read)
{
    m_readReplies = read; updateUnread({}, m_local);
    for (auto it = m_peers.begin(); it != m_peers.end(); ++it) updateUnread(it.key(), it->box);
}
QJsonObject FleetState::unreadReplies() const
{
    QJsonObject replies;
    const auto collect = [&](const QString &host, const BoxState &box) {
        for (const auto &s : box.sessions)
            if (s.unreadReply) replies[replyKey(host, s)] = s.replyId;
    };
    collect({}, m_local);
    for (auto it = m_peers.cbegin(); it != m_peers.cend(); ++it) collect(it.key(), it->box);
    return replies;
}
int FleetState::markRepliesRead(const QJsonObject &replies)
{
    int marked = 0;
    const auto mark = [&](const QString &host, BoxState &box) {
        for (const auto &s : box.sessions) {
            if (!s.unreadReply) continue;
            const auto key = replyKey(host, s);
            // A newer poll may have arrived since the UI took its snapshot.
            // Acknowledging old replies must never acknowledge the next answer.
            if (replies.value(key).toString() != s.replyId || m_readReplies.value(key).toString() == s.replyId) continue;
            m_readReplies[key] = s.replyId; ++marked;
        }
        updateUnread(host, box);
    };
    mark({}, m_local);
    for (auto it = m_peers.begin(); it != m_peers.end(); ++it) mark(it.key(), it->box);
    return marked;
}
bool FleetState::markReplyRead(const QString &host, const QString &name, const QString &conversation, const QString &reply)
{
    if (reply.isEmpty() || conversation.isEmpty()) return false;
    BoxState *box = host.isEmpty() ? &m_local : m_peers.contains(host) ? &m_peers[host].box : nullptr;
    if (!box) return false;
    for (auto &s : box->sessions) {
        if (s.state == "archived" || s.name != name || s.conversationId != conversation || s.replyId != reply || !s.unreadReply) continue;
        m_readReplies[replyKey(host, s)] = reply;
        updateUnread(host, *box); return true;
    }
    return false;
}

void FleetState::setLocal(const BoxState &box, qint64 /*nowMs*/)
{
    // Свой бокс не имеет понятия "опрошен давно": мы и есть этот бокс, вызов hgs
    // локальный и синхронный по сути (миллисекунды). localIsAlwaysFresh() полагается
    // на то, что isStale() вообще не смотрит на m_local.
    m_local = box; updateUnread({}, m_local);
    m_haveLocal = true;
    if (box.peersKnown) {
        for (auto it = m_peers.begin(); it != m_peers.end(); ) {
            if (!box.peers.contains(it.key())) it = m_peers.erase(it); else ++it;
        }
        for (const auto &peer : box.peers) registerPeer(peer);
    }
}

void FleetState::registerPeer(const QString &alias)
{
    // operator[] на QMap создаёт запись со значениями по умолчанию, если её ещё нет;
    // если пир уже был опрошен раньше — не затираем его состояние повторной регистрацией.
    m_peers[alias];
}

void FleetState::setPeer(const BoxState &box, qint64 nowMs)
{
    // A late reply cannot resurrect a profile removed/disabled while polling.
    if (m_local.peersKnown && !m_local.peers.contains(box.host)) return;
    PeerEntry &entry = m_peers[box.host];

    if (box.ok) {
        entry.box = box; updateUnread(box.host, entry.box);
    } else {
        // Пир ответил "я офлайн/сплю" — это не то же самое, что "мы не знаем, что там".
        // hgs в таком случае не может опросить tmux и присылает sessions:[], но реальные
        // сессии на ноуте никуда не делись. Берём из нового ответа только то, что он
        // ДЕЙСТВИТЕЛЬНО знает (host/ok/error), и оставляем sessions/projects от последнего
        // успешного опроса — иначе totalSessions() соврёт вниз ровно в момент засыпания.
        entry.box.host = box.host;
        entry.box.ok = false;
        entry.box.error = box.error;
    }

    // polledAt обновляется всегда, даже на ok:false: мы СПРОСИЛИ и получили осмысленный
    // ответ (пир жив и сказал "не сейчас"), это не то же самое, что молчание/таймаут
    // самого hgs (тот идёт через HgsClient::failed и вообще не долетает до setPeer).
    // Только так тултип может отличить "спросили только что, недоступен" от "не спрашивали
    // час" — см. tooltip().
    entry.polledAt = nowMs;
    entry.everPolled = true;
}

int FleetState::totalSessions() const
{
    int total = m_haveLocal ? visibleSessionCount(m_local) : 0;
    for (auto it = m_peers.constBegin(); it != m_peers.constEnd(); ++it) {
        // Не фильтруем по box.ok: последний известный список сессий переживает переход
        // в ok:false (см. setPeer) именно затем, чтобы здесь его не пришлось откидывать.
        // Фильтр только по everPolled — пока пира не опросили ни разу, его сессии
        // неизвестны, а не нулевые, и складывать в сумму нечего.
        if (it->everPolled)
            total += visibleSessionCount(it->box);
    }
    return total;
}

bool FleetState::isStale(qint64 nowMs) const
{
    // Свой бокс никогда не протухает: опрос локальный и всегда актуален на момент вызова.
    for (auto it = m_peers.constBegin(); it != m_peers.constEnd(); ++it) {
        const PeerEntry &e = it.value();
        if (!e.everPolled)
            return true;
        if (nowMs - e.polledAt > kPeerStaleMs)
            return true;
        if (!e.box.ok)
            return true;
    }
    return false;
}

int FleetState::attentionSessions(qint64 nowMs) const
{
    auto count = [](const BoxState &box) {
        return int(std::count_if(box.sessions.cbegin(), box.sessions.cend(),
            [&box](const SessionInfo &s) { return s.reviewLater || (box.ok && s.needsAttention()); }));
    };
    int total = m_haveLocal ? count(m_local) : 0;
    for (const auto &entry : m_peers)
        if (entry.everPolled) {
            if (nowMs - entry.polledAt < kPeerStaleMs) total += count(entry.box);
            else total += int(std::count_if(entry.box.sessions.cbegin(), entry.box.sessions.cend(),
                [](const SessionInfo &s) { return s.reviewLater; }));
        }
    return total;
}

QString FleetState::tooltip(qint64 nowMs) const
{
    QStringList lines;
    lines << QStringLiteral("%1 need attention, %2 sessions total").arg(attentionSessions(nowMs)).arg(totalSessions());
    lines << QStringLiteral("%1 here").arg(m_haveLocal ? visibleSessionCount(m_local) : 0);

    for (auto it = m_peers.constBegin(); it != m_peers.constEnd(); ++it) {
        const QString &alias = it.key();
        const PeerEntry &e = it.value();

        if (!e.everPolled) {
            lines << QStringLiteral("%1: not polled yet").arg(alias);
            continue;
        }

        const QString age = formatAge(nowMs - e.polledAt);
        if (!e.box.ok) {
            // "unreachable" само по себе не отвечает на вопрос "а мы вообще спрашивали
            // недавно?" — а это ровно то, чем спящий ноут (свежий отказ) отличается от
            // потерянного контакта (старый отказ). Возраст последнего опроса — тот
            // самый ответ.
            lines << QStringLiteral("%1: unreachable (checked %2)").arg(alias, age);
            continue;
        }

        lines << QStringLiteral("%1 on %2 (%3)").arg(visibleSessionCount(e.box)).arg(alias, age);
    }

    return lines.join(QLatin1Char('\n'));
}

QList<QString> FleetState::peerNames() const
{
    return m_peers.keys();
}

const BoxState *FleetState::peer(const QString &alias) const
{
    const auto it = m_peers.constFind(alias);
    if (it == m_peers.constEnd())
        return nullptr;
    return &it->box;
}

qint64 FleetState::peerPolledAt(const QString &alias) const
{
    const auto it = m_peers.constFind(alias);
    if (it == m_peers.constEnd())
        return 0;
    return it->polledAt;
}
