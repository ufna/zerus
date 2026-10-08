#include "ComposerDraftStore.h"

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSet>
#include <QSettings>

namespace {
const auto PrivateDirectory = QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner;
const auto PrivateFile = QFile::ReadOwner | QFile::WriteOwner;
QString hash(const QByteArray &bytes) { return QString::fromLatin1(QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex()); }
bool atomicWrite(const QString &path, const QByteArray &bytes, QString &error)
{
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || !file.setPermissions(PrivateFile)
        || file.write(bytes) != bytes.size() || !file.commit()) {
        error = QObject::tr("Draft not saved: %1. Keep this window open until saving succeeds.").arg(file.errorString());
        return false;
    }
    return true;
}
}

QString ComposerDraftStore::directory()
{
    // QSettings also provides fixture isolation on Linux and macOS without
    // changing existing organization/application identifiers or user paths.
    return QSettings().fileName() + QStringLiteral(".drafts");
}

QString ComposerDraftStore::folder(const QString &key) const { return directory() + '/' + hash(key.toUtf8()); }

ComposerDraft ComposerDraftStore::load(const QString &key) const
{
    ComposerDraft draft;
    QFile file(folder(key) + "/draft.json");
    if (!file.exists()) return draft;
    auto invalid = [&draft] {
        draft.storageError = QObject::tr("Could not restore this draft completely. Saved files have been kept locally.");
    };
    if (!file.open(QIODevice::ReadOnly)) { invalid(); return draft; }
    QJsonParseError error;
    const auto object = QJsonDocument::fromJson(file.readAll(), &error).object();
    if (error.error != QJsonParseError::NoError || object.value("version").toInt() != 1 || object.value("key").toString() != key) {
        invalid(); return draft;
    }
    draft.text = object.value("text").toString();
    draft.label = object.value("label").toString(); draft.formState = object.value("form_state").toObject();
    draft.position = object.value("position").toInt(); draft.anchor = object.value("anchor").toInt();
    draft.nextAttachmentNumber = qMax(1, object.value("next_attachment_number").toInt(1));
    draft.uncertain = object.value("uncertain").toBool() || object.value("sending").toBool();
    draft.error = draft.uncertain;
    if (draft.uncertain) draft.notice = QObject::tr("Delivery was interrupted. Check Activity or Terminal before sending again.");
    qsizetype total = 0;
    for (const auto &value : object.value("attachments").toArray()) {
        const auto attachment = value.toObject();
        const auto digest = attachment.value("sha256").toString();
        if (digest.size() != 64 || digest.contains(QRegularExpression("[^0-9a-f]"))
            || draft.attachments.size() >= HgsClient::MaximumAttachments) { invalid(); break; }
        QFile bytesFile(folder(key) + '/' + digest + ".bin");
        if (!bytesFile.open(QIODevice::ReadOnly) || bytesFile.size() > HgsClient::MaximumAttachmentBytes) { invalid(); continue; }
        const auto bytes = bytesFile.read(HgsClient::MaximumAttachmentBytes + 1);
        total += bytes.size();
        if (bytes.isEmpty() || total > HgsClient::MaximumAttachmentsBytes || hash(bytes) != digest
            || bytes.size() != attachment.value("bytes").toInteger()) { invalid(); continue; }
        draft.attachments.append({attachment.value("name").toString(), attachment.value("mime").toString(), bytes, attachment.value("reference").toString()});
        draft.attachmentHashes.append(digest);
    }
    return draft;
}

bool ComposerDraftStore::save(const QString &key, ComposerDraft &draft) const
{
    if (key.isEmpty()) return true;
    const auto path = folder(key);
    if (!QDir().mkpath(path) || !QFile::setPermissions(directory(), PrivateDirectory) || !QFile::setPermissions(path, PrivateDirectory)) {
        draft.storageError = QObject::tr("Draft not saved: cannot create private draft storage. Keep this window open until saving succeeds.");
        return false;
    }
    if (draft.attachmentHashes.size() != draft.attachments.size()) {
        draft.attachmentHashes.clear();
        for (const auto &attachment : draft.attachments) draft.attachmentHashes.append(hash(attachment.data));
    }
    QJsonArray attachments;
    QSet<QString> retained;
    for (int i = 0; i < draft.attachments.size(); ++i) {
        const auto &attachment = draft.attachments[i]; const auto digest = draft.attachmentHashes[i];
        const auto name = digest + ".bin"; retained.insert(name);
        if (!QFile::exists(path + '/' + name) && !atomicWrite(path + '/' + name, attachment.data, draft.storageError)) return false;
        attachments.append(QJsonObject{{"name", attachment.name}, {"mime", attachment.mime}, {"reference", attachment.reference},
                                      {"sha256", digest}, {"bytes", attachment.data.size()}});
    }
    const QJsonObject object{{"version", 1}, {"key", key}, {"text", draft.text}, {"position", draft.position}, {"anchor", draft.anchor},
                            {"label", draft.label}, {"form_state", draft.formState},
                            {"next_attachment_number", draft.nextAttachmentNumber}, {"sending", draft.sending}, {"uncertain", draft.uncertain},
                            {"attachments", attachments}};
    if (!atomicWrite(path + "/draft.json", QJsonDocument(object).toJson(QJsonDocument::Compact), draft.storageError)) return false;
    draft.storageError.clear();
    // The committed manifest is authoritative, including an empty tombstone
    // after success. A crash during cleanup cannot resurrect sent attachments.
    for (const auto &name : QDir(path).entryList({"*.bin"}, QDir::Files))
        if (!retained.contains(name)) QFile::remove(path + '/' + name);
    return true;
}

QStringList ComposerDraftStore::keys() const
{
    QStringList result;
    for (const auto &name : QDir(directory()).entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
        QFile file(directory() + '/' + name + "/draft.json");
        if (!file.open(QIODevice::ReadOnly)) continue;
        const auto object = QJsonDocument::fromJson(file.readAll()).object();
        const auto key = object.value("key").toString();
        if (object.value("version").toInt() == 1 && name == hash(key.toUtf8())
            && (!object.value("text").toString().isEmpty() || !object.value("attachments").toArray().isEmpty())) result.append(key);
    }
    result.sort(); return result;
}

QString ComposerDraftStore::label(const QString &key) const
{
    QFile file(folder(key) + "/draft.json");
    if (file.open(QIODevice::ReadOnly)) {
        const auto label = QJsonDocument::fromJson(file.readAll()).object().value("label").toString();
        if (!label.isEmpty()) return label;
    }
    return QString(key).replace('\n', " / ");
}
