#!/usr/bin/env python3
"""Temporary, exact-artifact input for the unmerged Windows crash diagnostic."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import tempfile
import time
import urllib.parse
import urllib.request
import zipfile


REPOSITORY = "ufna/zerus"
SOURCE = "dd58e0a272f8f5d5c7bb1ee550b23ae525e57903"
RUN = 41439457918
ARTIFACT = 13141249891
SIZE = 56496469
DIGEST = "0419a26468478a2a73b50830015bc3e5f3f4edca409bacae5a5adf40eefc1924"
PORTABLE_SIZE = 33397959
PORTABLE_DIGEST = "8042cf399a874c5eabd8a6b7c5ed43726edbfabc642861660f78ad560da46c98"
FILES = {
    "hgs.exe": "f3828965a365b2e94ad296f79b9fae2e575dee4f26260243620164e6ed6058d8",
    "vcruntime140.dll": "d1f4225df2cd877dbf130d5668a021dce3f94118455ff5ec952061c30afc9ce7",
    "vcruntime140_1.dll": "a7146c08f89fe5b04541ab507cdb59ff7b44534d4ba3c668a426c6450a03434e",
    "msvcp140.dll": "7c26614e1d733892c2deac7e245ce115504b1d80592dd0a01b08e3e5a55f89ca",
}


class PublicRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, url):
        if urllib.parse.urlsplit(url).scheme != "https":
            raise ValueError("artifact redirect must use HTTPS")
        redirected = super().redirect_request(request, fp, code, message, headers, url)
        # The signed storage URL needs no repository token. Never forward it.
        redirected.remove_header("Authorization")
        return redirected


def request(path, token):
    return urllib.request.Request(
        "https://api.github.com/repos/" + REPOSITORY + path,
        headers={"Authorization": "Bearer " + token,
                 "Accept": "application/vnd.github+json", "User-Agent": "Zerus-CI"},
    )


def metadata(opener, path, token):
    with opener.open(request(path, token), timeout=30) as response:
        body = response.read(1024 * 1024 + 1)
    if len(body) > 1024 * 1024:
        raise ValueError("bounded artifact metadata required")
    return json.loads(body)


def prepare(archive, output):
    if output.exists():
        raise ValueError("diagnostic output must be new")
    with zipfile.ZipFile(archive) as outer:
        if len(outer.infolist()) > 256:
            raise ValueError("bounded package artifact required")
        entry = outer.getinfo("zerus-windows-x86_64.zip")
        if entry.file_size != PORTABLE_SIZE:
            raise ValueError("portable size differs")
        portable = outer.read(entry)
    if hashlib.sha256(portable).hexdigest() != PORTABLE_DIGEST:
        raise ValueError("portable digest differs")
    with zipfile.ZipFile(io.BytesIO(portable)) as inner:
        names = inner.namelist()
        if len(names) > 4096 or len(set(names)) != len(names):
            raise ValueError("bounded unique portable entries required")
        selected = {}
        for name, digest in FILES.items():
            entry = inner.getinfo("zerus-windows-x86_64/" + name)
            if entry.file_size > 64 * 1024 * 1024:
                raise ValueError("image size exceeds bound")
            body = inner.read(entry)
            if hashlib.sha256(body).hexdigest() != digest:
                raise ValueError("exact retained image differs: " + name)
            selected[name] = body
    output.mkdir()
    for layout in ("raw", "app-local"):
        directory = output / layout / "release"
        directory.mkdir(parents=True)
        for name, body in selected.items():
            if layout == "app-local" or name == "hgs.exe":
                (directory / name).write_bytes(body)
    print(json.dumps({"source": SOURCE, "run": RUN, "artifact": ARTIFACT,
                      "sha256": FILES, "layouts": ["raw", "app-local"]}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if (os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or os.environ.get("GITHUB_REPOSITORY") != REPOSITORY):
        raise ValueError("disposable upstream hosted runner required")
    token = os.environ["GH_TOKEN"]  # Job's own read-only token, never persisted.
    opener = urllib.request.build_opener(PublicRedirect())
    value = metadata(opener, f"/actions/artifacts/{ARTIFACT}", token)
    if (value["id"] != ARTIFACT or value["expired"]
            or value["name"] != "windows-x86_64-" + SOURCE
            or value["workflow_run"]["id"] != RUN
            or value["workflow_run"]["head_sha"] != SOURCE
            or value["size_in_bytes"] != SIZE or value["digest"] != "sha256:" + DIGEST):
        raise ValueError("retained artifact identity differs")
    jobs = metadata(opener, f"/actions/runs/{RUN}/jobs?per_page=100", token)
    if jobs["total_count"] > 100:
        raise ValueError("job metadata exceeds bound")
    for suffix in ("Quick checks (source and Rust 1.99)",
                   "Desktop client native checks (windows-2025)",
                   "Windows client, desktop and portable zip"):
        matches = [job for job in jobs["jobs"] if job["name"].endswith(suffix)]
        if (len(matches) != 1 or matches[0]["status"] != "completed"
                or matches[0]["conclusion"] != "success"):
            raise ValueError("retained source/native/package qualification differs")
    with tempfile.TemporaryDirectory(prefix="zerus-registration-download-") as temporary:
        archive = Path(temporary) / "artifact.zip"
        total = 0
        digest = hashlib.sha256()
        deadline = time.monotonic() + 90
        with opener.open(request(f"/actions/artifacts/{ARTIFACT}/zip", token), timeout=30) as response, archive.open("xb") as target:
            while chunk := response.read(1024 * 1024):
                total += len(chunk)
                if total > SIZE or time.monotonic() > deadline:
                    raise ValueError("bounded retained artifact download required")
                digest.update(chunk)
                target.write(chunk)
        if total != SIZE or digest.hexdigest() != DIGEST:
            raise ValueError("retained artifact bytes differ")
        prepare(archive, args.output)


if __name__ == "__main__":
    main()
