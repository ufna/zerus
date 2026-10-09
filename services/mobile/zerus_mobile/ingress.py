"""Bounded transport admission and JSON processing for the inline v1 protocol."""
from __future__ import annotations

import asyncio
import ipaddress
import codecs
import json
import tempfile

from aiohttp import web

from .work import blocking


def busy(message="relay capacity is busy"):
    return web.HTTPTooManyRequests(text=json.dumps({"error": message}), content_type="application/json", headers={"Retry-After": "1"})


class Capacity:
    """Fail fast without creating semaphore waiters; event-loop-owned counters."""
    def __init__(self, limit):
        self.limit, self.used = limit, 0

    def take(self, amount=1):
        if amount > self.limit - self.used:
            raise busy()
        self.used += amount

    def release(self, amount=1):
        self.used -= amount


def client_identity(request, cfg):
    peer = request.remote or "unknown"
    try:
        address = ipaddress.ip_address(peer)
    except ValueError:
        return peer
    trusted = tuple(ipaddress.ip_network(value) for value in cfg.trusted_proxy_cidrs)
    def is_trusted(value):
        return any(value in network for network in trusted)
    if not is_trusted(address):
        return str(address)
    forwarded = request.headers.get("X-Forwarded-For")
    if forwarded is None:
        return str(address)
    if len(forwarded) > 2048:
        raise web.HTTPBadRequest()
    chain = forwarded.split(",")
    if not 1 <= len(chain) <= cfg.max_forwarded_hops:
        raise web.HTTPBadRequest()
    try:
        chain = [ipaddress.ip_address(item.strip()) for item in chain]
    except ValueError:
        raise web.HTTPBadRequest() from None
    for candidate in reversed(chain):
        if not is_trusted(address):
            break
        address = candidate
    return str(address)


def parse_json(raw, max_depth, max_values, max_string_memory=48 * 1024 * 1024):
    """Count structure before json.loads can amplify dense arrays into objects.

    This lexical pass allocates no token strings. json.loads remains responsible
    for the full grammar, UTF-8, escapes and scalar validation.
    """
    depth = values = 0
    quoted = escaped = scalar = False
    scalar_size = string_size = 0
    wide = False
    unicode_left = unicode_value = string_memory = 0
    for char in raw:
        if quoted:
            string_size += 1
            if char < 32:
                raise ValueError("unescaped JSON control character")
            if char >= 128:
                wide = True
            if unicode_left:
                if 48 <= char <= 57: digit = char - 48
                elif 65 <= char <= 70: digit = char - 55
                elif 97 <= char <= 102: digit = char - 87
                else: raise ValueError("invalid JSON unicode escape")
                unicode_value = unicode_value * 16 + digit
                unicode_left -= 1
                if not unicode_left and unicode_value >= 128:
                    wide = True
            if wide and string_size > 1024 * 1024:
                raise ValueError("non-ASCII JSON string exceeds limit")
            if escaped:
                escaped = False
                if char == 117:
                    unicode_left = 4
                    unicode_value = 0
            elif char == 92:
                escaped = True
            elif char == 34:
                quoted = False
                # Conservative Python string width estimate; escapes that decode
                # ASCII stay narrow. This also bounds many individually small
                # mixed-width strings before materializing their objects.
                string_memory += string_size * (4 if wide else 1)
                if string_memory > max_string_memory:
                    raise ValueError("JSON string memory exceeds limit")
            continue
        if char == 34:
            quoted = True
            string_size = 0
            wide = False
            values += 1
            scalar = False
        elif char in (91, 123):
            depth += 1
            values += 1
            scalar = False
            if depth > max_depth:
                raise ValueError("JSON nesting exceeds limit")
        elif char in (93, 125):
            depth -= 1
            scalar = False
            if depth < 0:
                raise ValueError("invalid JSON nesting")
        elif char in (9, 10, 13, 32, 44, 58):
            scalar = False
        else:
            if not scalar:
                values += 1
                scalar_size = 0
                scalar = True
            scalar_size += 1
            if scalar_size > 4300:
                raise ValueError("JSON scalar exceeds limit")
        if values > max_values:
            raise ValueError("JSON structure exceeds limit")
    if quoted or depth:
        raise ValueError("unterminated JSON")
    return _parse_tokens(raw)


def _parse_tokens(raw):
    """Decode individual strings, never a whole mixed-width Unicode document."""
    length = len(raw)
    position = 0
    def whitespace():
        nonlocal position
        while position < length and raw[position] in (9, 10, 13, 32):
            position += 1
    def string():
        nonlocal position
        start = position + 1
        position = start
        escaped = False
        while True:
            end = raw.find(b'"', position)
            if end < 0: raise ValueError("unterminated JSON string")
            slash = end - 1
            while slash >= start and raw[slash] == 92: slash -= 1
            if (end - 1 - slash) % 2:
                position = end + 1
                escaped = True
                continue
            break
        position = end + 1
        # A released view cannot keep a large input alive through an error's
        # traceback, and decoding plain ASCII needs no full token byte copy.
        with memoryview(raw)[start:end] as view:
            if not escaped and 92 not in view and all(char < 128 and char >= 32 for char in view):
                return codecs.decode(view, "ascii")
        return json.loads(raw[start - 1:end + 1])
    def value():
        nonlocal position
        whitespace()
        if position >= length: raise ValueError("missing JSON value")
        token = raw[position]
        if token == 34:
            return string()
        if token == 123:
            position += 1
            result = {}
            whitespace()
            if position < length and raw[position] == 125:
                position += 1
                return result
            while True:
                whitespace()
                if position >= length or raw[position] != 34: raise ValueError("invalid JSON object key")
                key = string()
                whitespace()
                if position >= length or raw[position] != 58: raise ValueError("missing JSON colon")
                position += 1
                result[key] = value()
                whitespace()
                if position < length and raw[position] == 125:
                    position += 1
                    return result
                if position >= length or raw[position] != 44: raise ValueError("missing JSON comma")
                position += 1
        if token == 91:
            position += 1
            result = []
            whitespace()
            if position < length and raw[position] == 93:
                position += 1
                return result
            while True:
                result.append(value())
                whitespace()
                if position < length and raw[position] == 93:
                    position += 1
                    return result
                if position >= length or raw[position] != 44: raise ValueError("missing JSON comma")
                position += 1
        start = position
        while position < length and raw[position] not in (9, 10, 13, 32, 44, 93, 125):
            position += 1
        def nonfinite(_): raise ValueError("nonfinite JSON")
        return json.loads(raw[start:position], parse_constant=nonfinite)
    try:
        result = value()
        whitespace()
        if position != length: raise ValueError("trailing JSON data")
        return result
    finally:
        # Recursive nested functions capture their own closure cell. Explicitly
        # break that cycle so every parsed raw body is freed immediately, even
        # with cyclic GC disabled and after invalid JSON.
        value = string = whitespace = None
        raw = None


def _encoded(value):
    """An encoder whose largest temporary string is bounded even for base64."""
    if isinstance(value, str):
        yield b'"'
        for start in range(0, len(value), 8192):
            yield json.dumps(value[start:start + 8192], ensure_ascii=True)[1:-1].encode("ascii")
        yield b'"'
    elif isinstance(value, dict):
        yield b"{"
        for index, (key, item) in enumerate(value.items()):
            if index: yield b","
            yield from _encoded(key)
            yield b":"
            yield from _encoded(item)
        yield b"}"
    elif isinstance(value, (list, tuple)):
        yield b"["
        for index, item in enumerate(value):
            if index: yield b","
            yield from _encoded(item)
        yield b"]"
    else:
        yield json.dumps(value, allow_nan=False).encode("ascii")



def encoded_size(value, maximum):
    """Stop counting at the limit without creating a whole serialized copy."""
    total = 0
    for chunk in _encoded(value):
        total += len(chunk)
        if total > maximum:
            return total
    return total


def _spool(value, maximum):
    # TemporaryFile uses mode 0600 and unlinks immediately where supported.
    target = tempfile.TemporaryFile()
    try:
        total = 0
        for chunk in _encoded(value):
            total += len(chunk)
            if total > maximum:
                raise web.HTTPRequestEntityTooLarge(max_size=maximum, actual_size=total)
            target.write(chunk)
        target.seek(0)
        return target
    except BaseException:
        target.close()
        raise


async def stream_json(request, value, status=200, *, budget=None, maximum=4 * 1024 * 1024):
    """Spool bounded chunks off-loop, then send without a full output copy."""
    if budget is not None:
        budget.take(maximum)
    target = None
    try:
        target = await blocking(_spool, value, maximum)
        response = web.StreamResponse(status=status, headers={"Content-Type": "application/json", "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff"})
        await response.prepare(request)
        while True:
            chunk = await blocking(target.read, 65536)
            if not chunk:
                break
            await response.write(chunk)
        await response.write_eof()
        return response
    finally:
        if target is not None:
            target.close()
        if budget is not None:
            budget.release(maximum)
