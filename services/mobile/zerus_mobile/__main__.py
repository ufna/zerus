"""Offline administration and relay entrypoint. Administration has no HTTP API."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import stat

from aiohttp import web

from .server import Config, create_app
from .store import Store


def private_file(path):
    path = Path(path)
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) & 0o077:
        raise ValueError(f"private file must have mode 0600: {path}")
    return path


def main(argv=None):
    parser = argparse.ArgumentParser(description="Zerus trusted mobile relay (TLS transport, no E2EE)")
    parser.add_argument("--database", default=str(Path.home() / ".local/share/zerus-mobile/relay.sqlite3"))
    commands = parser.add_subparsers(dest="command", required=True)
    provision = commands.add_parser("provision", help="Create workspace, computer credential and one-time phone invitation")
    provision.add_argument("--name", required=True)
    provision.add_argument("--computer-name", default="Computer")
    node = commands.add_parser("node", help="Add a computer credential")
    node.add_argument("--workspace", required=True)
    node.add_argument("--name", required=True)
    invitation = commands.add_parser("invite", help="Create a ten-minute, single-use phone invitation")
    invitation.add_argument("--workspace", required=True)
    for command in ("revoke-device", "revoke-node"):
        commands.add_parser(command).add_argument("--id", required=True)
    serve = commands.add_parser("serve")
    serve.add_argument("--host", default="127.0.0.1")
    serve.add_argument("--port", type=int, default=8787)
    serve.add_argument("--config", help="Private JSON config: push_hosts, fcm_credentials, timeout/queue settings")
    args = parser.parse_args(argv)
    os.umask(0o077)
    store = Store(args.database)
    try:
        if args.command == "provision":
            workspace = store.workspace(args.name)
            result = {"workspace_id": workspace, **store.node(workspace, args.computer_name), **store.invite(workspace)}
        elif args.command == "node":
            result = {"workspace_id": args.workspace, **store.node(args.workspace, args.name)}
        elif args.command == "invite":
            result = {"workspace_id": args.workspace, **store.invite(args.workspace)}
        elif args.command in ("revoke-device", "revoke-node"):
            role = "devices" if args.command == "revoke-device" else "nodes"
            if not store.revoke(role, args.id):
                parser.error("credential not found")
            result = {"revoked": True}
        else:
            options = {}
            if args.config:
                options = json.loads(private_file(args.config).read_text())
                if not isinstance(options, dict) or set(options) - {"queue_ttl", "claim_ttl", "retention", "max_queue", "max_queue_bytes", "online_timeout", "push_hosts", "fcm_credentials"}:
                    parser.error("invalid relay configuration")
            if options.get("fcm_credentials"):
                options["fcm_credentials"] = str(private_file(options["fcm_credentials"]))
            if "push_hosts" in options:
                if not isinstance(options["push_hosts"], list) or any(not isinstance(h, str) or not h or any(c in h for c in "/:@* ") for h in options["push_hosts"]):
                    parser.error("push_hosts must list exact hostnames")
                options["push_hosts"] = tuple(options["push_hosts"])
            for key in ("queue_ttl", "claim_ttl", "retention", "max_queue", "max_queue_bytes", "online_timeout"):
                if key in options and (type(options[key]) is not int or options[key] <= 0):
                    parser.error(f"{key} must be a positive integer")
            # access_log would leak request/session/endpoint metadata. Proxy logging
            # must also be disabled by the deployment administrator.
            web.run_app(create_app(store, Config(**options)), host=args.host, port=args.port, access_log=None, handler_args={"max_headers": 100, "max_field_size": 8190, "keepalive_timeout": 30, "auto_decompress": False})
            return
        # Deliberate one-time credential output, never server logging.
        print(json.dumps(result))
    finally:
        store.close()


if __name__ == "__main__":
    main()
