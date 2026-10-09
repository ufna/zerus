"""Exact managed relay transport migration; stored credentials keep their binding."""
from urllib.parse import urlsplit

CURRENT = "https://relay.zerus.dev"
LEGACY = "https://zerus.dev.guthub.dev"


def managed_origin(value: str) -> str | None:
    try:
        url = urlsplit(value)
        if (url.scheme != "https" or url.port not in (None, 443)
                or url.username is not None or url.password is not None
                or url.query or url.fragment or url.path not in ("", "/")):
            return None
        return {"zerus.dev.guthub.dev": LEGACY, "relay.zerus.dev": CURRENT}.get(url.hostname)
    except ValueError:
        return None


def transport(value: str) -> str:
    return CURRENT if managed_origin(value) == LEGACY else value


def identity_url(server_url: str, explicit: str | None = None) -> str:
    if explicit is None or explicit == server_url:
        return server_url
    if managed_origin(server_url) == CURRENT and managed_origin(explicit) == LEGACY:
        return explicit
    raise ValueError("identity_url may only preserve the previous official relay binding")
