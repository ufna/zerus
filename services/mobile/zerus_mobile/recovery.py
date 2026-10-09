"""Narrow waiting-job control, never a global recovery policy editor."""
import uuid
from .context import IDENTITY_FIELDS, bounded


def validate_recovery(payload, request_id):
    if not isinstance(payload,dict) or set(payload) != IDENTITY_FIELDS | {'job_id','action'} or payload.get('request_id') != request_id:
        raise ValueError('invalid scoped recovery fields')
    bounded(payload['expected_run_id'],128)
    bounded(payload['expected_conversation_id'],256)
    value=bounded(payload['job_id'],36)
    if str(uuid.UUID(value)) != value or uuid.UUID(value).int == 0: raise ValueError('invalid recovery job UUID')
    if payload['action'] not in {'now','cancel'}: raise ValueError('invalid recovery job action')
