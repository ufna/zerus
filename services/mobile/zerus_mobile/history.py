"""Bound public inspection history without cropping control disclosures."""
import json
import math

STREAMS = ("events", "message_events", "provider_messages", "attachment_messages")
USER_TYPES = {"UserPromptSubmit", "UserPromptQueued", "UserMessage", "TurnStarted", "QuestionAnswered"}


def encoded(value):
    return json.dumps(value, allow_nan=False).encode()


def completed_size(result):
    return len(encoded({"state": "completed", "result": result, "error": None}))


def bound_inspection(result, maximum):
    """Drop whole oldest history rows, preserving the newest own message first.

    All other fields retain their exact JSON values. ASCII escaping is counted
    as it is on the actual node-result HTTP upload. Run this CPU work off-loop.
    """
    if completed_size(result) <= maximum:
        return result
    value = dict(result)
    rows, latest = [], None
    for stream in STREAMS:
        if not isinstance(value.get(stream), list):
            continue
        for index, row in enumerate(value[stream]):
            at = row.get("at", 0) if isinstance(row, dict) else 0
            at = at if isinstance(at, (int, float)) and not isinstance(at, bool) and math.isfinite(at) else 0
            item = (at, stream, index, len(encoded(row)))
            rows.append(item)
            if (isinstance(row, dict) and row.get("type") in USER_TYPES
                    and row.get("agent_id", "") in ("", "main")
                    and (latest is None or (at, stream == "message_events") > (latest[0], latest[1] == "message_events"))):
                latest = item
    omitted = {stream: 0 for stream in STREAMS}
    value["mobile_history_truncated"] = True
    value["mobile_history_omitted"] = omitted
    size = completed_size(value)
    removed = {stream: set() for stream in STREAMS}
    remaining = {stream: len(value[stream]) for stream in STREAMS if isinstance(value.get(stream), list)}
    # Preserve one exact newest user row preferentially, then its loss is also
    # disclosed if mandatory metadata leaves no room even for that whole row.
    for item in sorted(rows, key=lambda item: (item == latest, item[0], item[1], item[2])):
        if size <= maximum:
            break
        _, stream, index, length = item
        before = len(str(omitted[stream]))
        omitted[stream] += 1
        size += len(str(omitted[stream])) - before
        size -= length + (2 if remaining[stream] > 1 else 0)
        remaining[stream] -= 1
        removed[stream].add(index)
    for stream, indices in removed.items():
        if indices:
            value[stream] = [row for index, row in enumerate(value[stream]) if index not in indices]
    if completed_size(value) > maximum:
        raise ValueError("native inspection metadata exceeds relay size limit; required disclosures were not cropped")
    return value


"""Exact-target read-only public history paging; native cursors remain opaque."""
import uuid
from .context import IDENTITY_FIELDS, bounded


def validate_history(payload, request_id):
    extras = {'archive_id','agent_id','limit','before','after','around','around_incoming_seq','history_epoch'}
    if not isinstance(payload,dict) or not IDENTITY_FIELDS <= set(payload) or set(payload)-IDENTITY_FIELDS-extras or payload.get('request_id') != request_id:
        raise ValueError('invalid exact history fields')
    bounded(payload['expected_run_id'],128)
    bounded(payload['expected_conversation_id'],256)
    if 'limit' in payload and (type(payload['limit']) is not int or not 1 <= payload['limit'] <= 100):
        raise ValueError('invalid history page limit')
    selectors = {'before','after','around','around_incoming_seq'} & set(payload)
    if len(selectors)>1: raise ValueError('choose one history selector')
    for field in ('before','after','around'):
        if field in payload: bounded(payload[field],2048)
    if ('around_incoming_seq' in payload) != ('history_epoch' in payload):
        raise ValueError('incoming anchor requires exact history epoch')
    if 'around_incoming_seq' in payload:
        if type(payload['around_incoming_seq']) is not int or not 0 <= payload['around_incoming_seq'] <= 2**63-1:
            raise ValueError('invalid incoming sequence')
        bounded(payload['history_epoch'],128)
    if 'agent_id' in payload:
        bounded(payload['agent_id'],160)
        if any(c in payload['agent_id'] for c in '/\\'): raise ValueError('invalid child identity')
    if 'archive_id' in payload:
        value=bounded(payload['archive_id'],36)
        if str(uuid.UUID(value)) != value or uuid.UUID(value).int == 0: raise ValueError('invalid archive identity')
