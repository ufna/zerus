"""Fixed raw Terminal transport; no paths, shell command construction or replay."""
from .context import IDENTITY_FIELDS, bounded

OPERATIONS = frozenset({"terminal_snapshot", "terminal_input"})
KEYS = frozenset({"Enter", "Escape", "Tab", "BTab", "BSpace", "Up", "Down", "Left", "Right", "Home", "End", "PPage", "NPage", "DC", "C-c", "C-d", "C-l", "C-a", "C-e", "C-u", "C-w"})


def validate(operation, payload, request_id):
    extras = {"terminal_binding_id", "text", "enter", "key"} if operation == "terminal_input" else set()
    if not isinstance(payload, dict) or not IDENTITY_FIELDS <= set(payload) or set(payload) - IDENTITY_FIELDS - extras or payload.get("request_id") != request_id:
        raise ValueError("invalid scoped terminal fields")
    bounded(payload["expected_run_id"], 128)
    bounded(payload["expected_conversation_id"], 256, empty=True)
    if operation == "terminal_input":
        binding = bounded(payload.get("terminal_binding_id"), 64)
        if len(binding) != 64 or any(c not in "0123456789abcdef" for c in binding):
            raise ValueError("invalid terminal binding")
        if "key" in payload:
            if set(payload) != IDENTITY_FIELDS | {"terminal_binding_id", "key"} or not isinstance(payload["key"], str) or payload["key"] not in KEYS:
                raise ValueError("unsupported terminal key")
        else:
            value = payload.get("text")
            if (set(payload) != IDENTITY_FIELDS | {"terminal_binding_id", "text", "enter"} or not isinstance(value, str)
                    or type(payload.get("enter")) is not bool or len(value.encode()) > 64 * 1024
                    or (not value and not payload["enter"]) or any(ord(c) < 32 and c not in "\n\t" or ord(c) == 127 for c in value)):
                raise ValueError("invalid literal terminal text")
