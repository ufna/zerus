"""Offline administration and relay entrypoint. Administration has no HTTP API."""

from __future__ import annotations

import argparse
import asyncio
from dataclasses import fields
import inspect
import json
import os
from pathlib import Path
import stat

from aiohttp import web

from .server import Config, create_app
from .store import Store

BACKEND_OPTIONS = {
    "database_url",
    "pool_min",
    "pool_max",
    "global_max_bytes",
    "quota_shards",
}


def private_file(path):
    path = Path(path)
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) & 0o077:
        raise ValueError(f"private file must have mode 0600: {path}")
    return path


def configuration(args, parser):
    options = json.loads(private_file(args.config).read_text()) if args.config else {}
    allowed = {field.name for field in fields(Config)} | BACKEND_OPTIONS
    if not isinstance(options, dict) or set(options) - allowed:
        parser.error("invalid relay configuration")
    if options.get("fcm_credentials"):
        options["fcm_credentials"] = str(private_file(options["fcm_credentials"]))
    if "push_hosts" in options:
        hosts = options["push_hosts"]
        if not isinstance(hosts, list) or any(
            not isinstance(h, str) or not h or any(c in h for c in "/:@* ")
            for h in hosts
        ):
            parser.error("push_hosts must list exact hostnames")
        options["push_hosts"] = tuple(hosts)
    if "trusted_proxy_cidrs" in options:
        import ipaddress

        try:
            for network in options["trusted_proxy_cidrs"]:
                ipaddress.ip_network(network)
        except (ValueError, TypeError):
            parser.error("invalid trusted_proxy_cidrs")
        options["trusted_proxy_cidrs"] = tuple(options["trusted_proxy_cidrs"])
    for key, value in options.items():
        if key not in {
            "push_hosts",
            "trusted_proxy_cidrs",
            "fcm_credentials",
            "database_url",
            "background",
        } and (
            type(value)
            not in ((int, float) if key == "maintenance_interval" else (int,))
            or value <= 0
        ):
            parser.error(f"{key} must be a positive number")
    if "database_url" in options and (
        not isinstance(options["database_url"], str)
        or not options["database_url"].startswith(("postgresql://", "postgres://"))
    ):
        parser.error("database_url must be a PostgreSQL URL")
    backend = {key: options.pop(key) for key in list(options) if key in BACKEND_OPTIONS}
    return options, backend


async def open_store(args, backend):
    if backend.get("database_url"):
        from .postgres import PostgresStore

        return await PostgresStore.open(
            backend["database_url"],
            **{key: value for key, value in backend.items() if key != "database_url"},
        )
    if backend:
        raise ValueError("PostgreSQL backend settings require database_url")
    return Store(args.database)


async def invoke(store, method, *args):
    value = getattr(store, method)(*args)
    return await value if inspect.isawaitable(value) else value


async def administer(args, backend, parser):
    store = await open_store(args, backend)
    try:
        if args.command == "provision":
            workspace = await invoke(store, "workspace", args.name)
            return {
                "workspace_id": workspace,
                **await invoke(store, "node", workspace, args.computer_name),
                **await invoke(store, "invite", workspace),
            }
        if args.command == "node":
            return {
                "workspace_id": args.workspace,
                **await invoke(store, "node", args.workspace, args.name),
            }
        if args.command == "invite":
            return {
                "workspace_id": args.workspace,
                **await invoke(store, "invite", args.workspace),
            }
        role = "devices" if args.command == "revoke-device" else "nodes"
        if not await invoke(store, "revoke", role, args.id):
            parser.error("credential not found")
        return {"revoked": True}
    finally:
        await invoke(store, "close")


async def application(args, options, backend):
    store = await open_store(args, backend)
    try:
        app = create_app(store, Config(**options))
        if isinstance(store, Store):

            async def close_sqlite(app):
                store.close()

            app.on_cleanup.append(close_sqlite)
        # create_app's lifecycle owns the async backend (and SQLite adapter).
        return app
    except BaseException:
        await invoke(store, "close")
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Zerus trusted mobile relay (TLS transport, no E2EE)"
    )
    parser.add_argument(
        "--database",
        default=str(Path.home() / ".local/share/zerus-mobile/relay.sqlite3"),
    )
    parser.add_argument(
        "--config",
        help="Private JSON configuration, including optional PostgreSQL database_url",
    )
    commands = parser.add_subparsers(dest="command", required=True)
    provision = commands.add_parser(
        "provision",
        help="Create workspace, computer credential and one-time phone invitation",
    )
    provision.add_argument("--name", required=True)
    provision.add_argument("--computer-name", default="Computer")
    node = commands.add_parser("node", help="Add a computer credential")
    node.add_argument("--workspace", required=True)
    node.add_argument("--name", required=True)
    invitation = commands.add_parser(
        "invite", help="Create a ten-minute, single-use phone invitation"
    )
    invitation.add_argument("--workspace", required=True)
    for command in ("revoke-device", "revoke-node"):
        commands.add_parser(command).add_argument("--id", required=True)
    serve = commands.add_parser("serve")
    serve.add_argument("--host", default="127.0.0.1")
    serve.add_argument("--port", type=int, default=8787)
    migrate = commands.add_parser(
        "migrate-sqlite",
        help="Atomically import an offline SQLite database into an empty PostgreSQL destination",
    )
    migrate.add_argument("--source", required=True)
    migrate.add_argument(
        "--source-offline",
        action="store_true",
        required=True,
        help="Confirm all source SQLite relay workers are stopped",
    )
    for command in (
        provision,
        node,
        invitation,
        serve,
        migrate,
        *[commands.choices[c] for c in ("revoke-device", "revoke-node")],
    ):
        command.add_argument(
            "--config",
            default=argparse.SUPPRESS,
            help="Private JSON relay configuration",
        )
    args = parser.parse_args(argv)
    os.umask(0o077)
    options, backend = configuration(args, parser)
    if args.command == "serve":
        # Proxy and application access logs must remain disabled. Disconnect
        # cancellation releases active poll leases and drains CPU/SQLite workers.
        web.run_app(
            application(args, options, backend),
            host=args.host,
            port=args.port,
            access_log=None,
            handler_cancellation=True,
            handler_args={
                "max_headers": 100,
                "max_field_size": 8190,
                "keepalive_timeout": 30,
                "auto_decompress": False,
            },
        )
        return
    if args.command == "migrate-sqlite":
        if not backend.get("database_url"):
            parser.error(
                "migration requires PostgreSQL database_url in private configuration"
            )
        from .migration import migrate_sqlite

        result = asyncio.run(
            migrate_sqlite(
                args.source,
                backend["database_url"],
                **{
                    key: value
                    for key, value in backend.items()
                    if key in {"global_max_bytes", "quota_shards"}
                },
            )
        )
    else:
        result = asyncio.run(administer(args, backend, parser))
    # Deliberate one-time credential output, never server logging.
    print(json.dumps(result))


if __name__ == "__main__":
    main()
