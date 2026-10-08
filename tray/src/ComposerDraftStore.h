#pragma once

#include "HgsClient.h"

struct ComposerDraft {
    QString text, notice, storageError, label;
    QJsonObject formState;
    QList<MessageAttachment> attachments;
    QStringList attachmentHashes;
    int nextAttachmentNumber = 1;
    int position = 0, anchor = 0;
    bool sending = false, error = false, uncertain = false;
};

// Private desktop state, deliberately separate from synchronized configuration.
// Each draft has an atomic manifest and immutable attachment snapshots, so typing
// never rewrites attachment bytes or relies on the original file staying around.
class ComposerDraftStore {
public:
    static QString directory();
    ComposerDraft load(const QString &key) const;
    bool save(const QString &key, ComposerDraft &draft) const;
    QStringList keys() const;
    QString label(const QString &key) const;
private:
    QString folder(const QString &key) const;
};
