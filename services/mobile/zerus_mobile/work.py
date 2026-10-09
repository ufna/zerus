"""Cancellation-safe bounded CPU work, independent of HTTP and storage drivers."""
from __future__ import annotations

import asyncio
from contextvars import ContextVar, copy_context
import functools

EXECUTOR = ContextVar("relay_payload_executor", default=None)


async def blocking(function, *args):
    """Keep caller admission until its actual executor operation completes."""
    loop = asyncio.get_running_loop()
    context = copy_context()
    task = loop.run_in_executor(EXECUTOR.get(), context.run, functools.partial(function, *args))
    try:
        return await asyncio.shield(task)
    except asyncio.CancelledError:
        while not task.done():
            try:
                await asyncio.shield(task)
            except asyncio.CancelledError:
                continue
            except Exception:
                break
        if task.done() and not task.cancelled():
            if task.exception() is None:
                result = task.result()
                if hasattr(result, "close"):
                    result.close()
        raise
    finally:
        # Future -> exception traceback -> this frame -> Future otherwise forms
        # a cycle that retains all worker arguments after a malformed payload.
        task = context = function = None
        args = ()
