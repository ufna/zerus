"""A minimal native logical-project view, without peer settings or CRDT history."""
from __future__ import annotations

import hashlib


def is_archived(row: dict) -> bool:
    archive_id = row.get("archive_id")
    return row.get("state") == "archived" or (isinstance(archive_id, str) and bool(archive_id.strip()))


def normalize(catalog: dict, snapshot: dict, fallback_id: str) -> dict:
    if not isinstance(catalog, dict) or catalog.get("schema") != 1 or not isinstance(catalog.get("organization"), dict):
        raise ValueError("invalid native project catalog")
    initialized = catalog.get("initialized") is True
    organization = catalog["organization"]
    groups = organization.get("projects")
    if not isinstance(groups, list) or len(groups) > 5000:
        raise ValueError("invalid native logical projects")
    node_id, swarm_id = catalog.get("node_id"), catalog.get("swarm_id")
    if not initialized:
        return unavailable(fallback_id)
    if not isinstance(node_id, str) or not node_id or not isinstance(swarm_id, str) or not swarm_id:
        raise ValueError("invalid native project identity")
    aliases = {snapshot["host"]} if isinstance(snapshot.get("host"), str) else set()
    for machine in catalog.get("machines", []):
        if isinstance(machine, dict) and machine.get("id") == node_id and isinstance(machine.get("connection"), str):
            aliases.add(machine["connection"])
    projects, ids = [], set()
    for group in groups:
        if not isinstance(group, dict) or not isinstance(group.get("id"), str) or not group["id"] or group["id"] in ids or not isinstance(group.get("name"), str) or not group["name"]:
            raise ValueError("invalid native project group")
        ids.add(group["id"])
        project = {"id": group["id"], "name": group["name"], "color": group.get("color", "#64b5f6"),
                   "accessible": group.get("accessible", True), "folders": [], "sessions": [], "archives": []}
        for folder in group.get("folders", []):
            if not isinstance(folder, dict) or folder.get("machine_id") != node_id:
                continue
            project["folders"].append({key: folder.get(key, "") for key in ("id", "name", "path", "machine_id", "machine_name")})
            project["folders"][-1]["local"] = True
        for membership in group.get("sessions", []):
            if not isinstance(membership, str) or "\n" not in membership:
                continue
            machine, session = membership.split("\n", 1)
            if machine not in aliases:
                continue
            if session.startswith("archive\n"):
                project["archives"].append(session.split("\n", 1)[1])
            elif session:
                project["sessions"].append(session)
        projects.append(project)
    default = organization.get("default_project")
    return {"schema": 1, "source": "swarm", "available": True, "stale": False,
            "swarm_id": swarm_id, "node_id": node_id,
            "default_project": default if default in ids else None, "projects": projects}


def unavailable(fallback_id: str) -> dict:
    return {"schema": 1, "source": "swarm", "available": False, "stale": False,
            "swarm_id": "local:" + hashlib.sha256(fallback_id.encode()).hexdigest(),
            "node_id": None, "default_project": None, "projects": []}


def apply_memberships(snapshot: dict, catalog: dict) -> None:
    if not catalog.get("available"):
        return
    sessions, archives = {}, {}
    for project in catalog["projects"]:
        for session in project["sessions"]:
            sessions.setdefault(session, project["id"])
        for archive in project["archives"]:
            archives.setdefault(archive, project["id"])
    for session in snapshot["sessions"]:
        if not isinstance(session, dict):
            continue
        group = archives.get(session.get("archive_id")) if is_archived(session) else sessions.get(session.get("name"))
        if group is None:
            group = catalog.get("default_project")
        if group is not None:
            session["mobile_project_id"] = group
