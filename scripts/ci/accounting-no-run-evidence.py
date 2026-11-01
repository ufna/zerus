"""Passive diagnostic evidence only. Never imports or executes a built artifact."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import struct
import sys

BASE = "cf3dc42d4cc6f47d51b2faaea6620463844ad288"
OVERLAY = "96c7491e555485fe6a0d8779e447c3540b3b0f66de582ee72cdce02a7f4a2157"
LOCK = "526c5d867a2dc92137157b24f6ddfc3d3ee807808e689623f324ff4d3f7effba"
MANIFEST = "23857a8f0e017e9e31bb313e43a0968d5f08289fb5639d42e52857365a33917d"
BRANCH = "refs/heads/ci/accounting-no-run-cf3"
RAW_CAP = 64 * 1024 * 1024
ELF_CAP = 256 * 1024 * 1024
LINE_CAP = 256 * 1024
JSON_CAP = 2 * 1024 * 1024
RETAINED = {"cargo-combined.raw": RAW_CAP, "pipeline-status.txt": 32,
            "capture.json": JSON_CAP, "context.json": JSON_CAP, "toolchain.json": JSON_CAP,
            "source-before.json": JSON_CAP, "source-after.json": JSON_CAP,
            "compiler-evidence.json": JSON_CAP, "hgs-test.elf": ELF_CAP,
            "hgs-test.elf.partial": ELF_CAP}


def bounded(path, cap):
    with open(path, "rb", buffering=0) as stream:
        data = stream.read(cap + 1)
    if len(data) > cap:
        raise ValueError("input capacity: " + str(path))
    return data


def hash_file(path, cap):
    h = hashlib.sha256()
    total = 0
    with open(path, "rb", buffering=0) as stream:
        while True:
            data = stream.read(min(65536, cap - total + 1))
            if not data:
                return h.hexdigest()
            total += len(data)
            if total > cap:
                raise ValueError("hash capacity")
            h.update(data)


def emit(path, value):
    data = json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode() + b"\n"
    if len(data) > JSON_CAP:
        raise ValueError("metadata capacity")
    with open(path, "xb", buffering=0) as stream:
        if stream.write(data) != len(data):
            raise OSError("short metadata write")
        os.fsync(stream.fileno())


def context():
    keys = ("GITHUB_REPOSITORY", "GITHUB_SHA", "GITHUB_WORKFLOW_SHA", "GITHUB_WORKFLOW_REF",
            "GITHUB_REF", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB",
            "GITHUB_EVENT_NAME", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_ENVIRONMENT",
            "ImageOS", "ImageVersion")
    data = {k: os.environ.get(k) for k in keys}
    sha = data["GITHUB_SHA"]
    if (os.environ.get("GITHUB_ACTIONS") != "true" or data["GITHUB_REPOSITORY"] != "ufna/zerus"
            or data["GITHUB_REF"] != BRANCH or data["GITHUB_EVENT_NAME"] != "workflow_dispatch"
            or data["GITHUB_RUN_ATTEMPT"] != "1" or data["GITHUB_JOB"] != "compiler-evidence"
            or data["RUNNER_OS"] != "Linux" or data["RUNNER_ARCH"] != "X64"
            or data["RUNNER_ENVIRONMENT"] != "github-hosted"
            or data["ImageOS"] != "ubuntu24" or not data["ImageVersion"]
            or not isinstance(sha, str) or re.fullmatch("[0-9a-f]{40}", sha) is None
            or data["GITHUB_WORKFLOW_SHA"] != sha or os.environ.get("INPUT_REF") != sha
            or data["GITHUB_WORKFLOW_REF"] != "ufna/zerus/.github/workflows/ci.yml@" + BRANCH
            or not str(data["GITHUB_RUN_ID"]).isdigit()):
        raise ValueError("exact hosted workflow/source/run binding")
    data.update(base=BASE, overlay=OVERLAY, job_database_id=None,
                job_database_id_requires_later_API_binding=True)
    names = ("BUILD_CARGO", "BUILD_RUSTC", "BUILD_RUSTDOC", "BUILD_PATH", "BUILD_HOME",
             "CARGO_HOME", "RUSTUP_HOME", "CARGO_TARGET_DIR", "BUILD_TMP", "BUILD_CONFIG",
             "BUILD_DATA", "BUILD_STATE", "BUILD_CACHE")
    data["selected_recipe_paths"] = {k: os.environ[k] for k in names}
    data["cargo_argv"] = [os.environ["BUILD_CARGO"], "test", "--locked", "--bin", "hgs",
                          "--no-run", "--message-format=json-render-diagnostics", "--verbose"]
    data["profile"] = dict(jobs=4, incremental=False, dev_debug=0, test_debug=0,
                           manifest_features_or_targets_added=False)
    return data


def source_snapshot(workspace, evidence):
    data = bounded(evidence / "tracked-paths.z", 1048576)
    if not data.endswith(b"\0"):
        raise ValueError("incomplete tracked path list")
    paths = data[:-1].split(b"\0", 8192)
    if not 1 <= len(paths) <= 8192 or len(set(paths)) != len(paths):
        raise ValueError("source path count/duplicate")
    rows = []
    total = 0
    for raw in sorted(paths):
        name = raw.decode("utf-8")
        p = PurePosixPath(name)
        if len(raw) > 4096 or p.is_absolute() or str(p) != name or ".." in p.parts:
            raise ValueError("tracked path grammar")
        path = workspace / name
        s = path.lstat()
        if not stat.S_ISREG(s.st_mode) or s.st_size > 64 * 1024 * 1024:
            raise ValueError("source type/size")
        total += s.st_size
        if total > 1024 * 1024 * 1024:
            raise ValueError("source total capacity")
        value = hash_file(path, s.st_size)
        after = path.lstat()
        attrs = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")
        if any(getattr(s, key) != getattr(after, key) for key in attrs):
            raise ValueError("source changed during snapshot")
        rows.append(dict(path=name, bytes=s.st_size, mode=stat.S_IMODE(s.st_mode), sha256=value))
    expected = {"src/state/creation_launch_status.rs": OVERLAY,
                "Cargo.lock": LOCK, "Cargo.toml": MANIFEST}
    by_name = {r["path"]: r for r in rows}
    if any(by_name.get(k, {}).get("sha256") != v for k, v in expected.items()):
        raise ValueError("overlay/manifest/lock changed")
    return dict(rows=rows, bytes=total, context=context())


def snapshot(workspace, evidence, phase):
    result = source_snapshot(workspace, evidence)
    emit(evidence / ("source-" + phase + ".json"), result)
    if phase == "before":
        emit(evidence / "context.json", context())
        rust = bounded(evidence / "rustc-version.txt", 65536).decode("utf-8")
        cargo = bounded(evidence / "cargo-version.txt", 65536).decode("utf-8")
        cfg = bounded(evidence / "compiler-cfg.txt", 65536).decode("utf-8").splitlines()
        if (not rust.startswith("rustc 1.99.1 ") or "host: x86_64-unknown-linux-gnu\n" not in rust
                or not cargo.startswith("cargo 1.99.1 ")
                or not {"test", 'target_os="linux"', 'target_arch="x86_64"', 'target_env="gnu"'} <= set(cfg)):
            raise ValueError("selected toolchain/test-target cfg")
        emit(evidence / "toolchain.json", dict(rustc_version=rust, cargo_version=cargo,
             queried_test_target_cfg=cfg, queried_cfg_is_not_complete_Cargo_command_capture=True,
             tools={key: dict(path=os.environ[key], sha256=hash_file(os.environ[key], 256 * 1024 * 1024))
                    for key in ("BUILD_CARGO", "BUILD_RUSTC", "BUILD_RUSTDOC", "RUSTUP_BIN")},
             cc_version=bounded(evidence / "cc-version.txt", 65536).decode("utf-8"),
             ar_version=bounded(evidence / "ar-version.txt", 65536).decode("utf-8")))


def capture(evidence):
    drained = written = synced = 0
    eof = overflow = False
    errors = []
    fd = None
    h = hashlib.sha256()
    try:
        fd = os.open(evidence / "cargo-combined.raw", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600)
        while True:
            data = sys.stdin.buffer.read(65536)
            if not data:
                eof = True
                break
            drained += len(data)
            if drained > 2**63 - 1:
                raise ValueError("drained counter overflow")
            if errors:
                continue  # no repair/retry; later drained bytes remain explicitly lost
            keep = data[:max(0, RAW_CAP - written)]
            try:
                if keep:
                    n = os.write(fd, keep)
                    written += n
                    h.update(keep[:n])
                    if n != len(keep):
                        raise OSError("short raw-prefix write")
                    os.fsync(fd)
                    synced = written
                if len(keep) != len(data):
                    overflow = True
                    raise ValueError("raw-prefix capacity")
            except Exception as error:
                errors.append(type(error).__name__ + ":" + str(error)[:160])
    except Exception as error:
        errors.append(type(error).__name__ + ":" + str(error)[:160])
    finally:
        if fd is not None:
            try:
                os.close(fd)  # consumed once, never retry the descriptor
            except OSError as error:
                errors.append("close:" + str(error)[:160])
        emit(evidence / "capture.json", dict(schema=1, stream="combined-cargo-stdout-stderr",
             drained=drained, written=written, synced=synced, sha256=h.hexdigest(),
             eof=eof, overflow=overflow, unknown_tail=not eof or overflow or bool(errors),
             errors=errors, complete=eof and not overflow and not errors))
    return 0 if eof and not overflow and not errors else 4


def cargo_record(evidence, workspace):
    candidates = []
    finished = []
    counts = dict(lines=0, json_records=0, non_json_diagnostic_lines=0)
    total = 0
    with open(evidence / "cargo-combined.raw", "rb", buffering=0) as stream:
        while True:
            line = stream.readline(min(LINE_CAP + 1, RAW_CAP - total + 1))
            if not line:
                break
            total += len(line)
            counts["lines"] += 1
            if total > RAW_CAP or counts["lines"] > 20000 or len(line) > LINE_CAP or not line.endswith(b"\n"):
                raise ValueError("record/line capacity or incomplete line")
            if not line.startswith(b"{"):
                counts["non_json_diagnostic_lines"] += 1
                continue
            obj = json.loads(line, parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))
            if not isinstance(obj, dict) or obj.get("reason") not in (
                    "compiler-artifact", "compiler-message", "build-script-executed", "build-finished"):
                raise ValueError("unknown Cargo JSON record")
            counts["json_records"] += 1
            if obj["reason"] == "build-finished":
                if type(obj.get("success")) is not bool:
                    raise ValueError("build-finished type")
                finished.append(obj["success"])
            if obj["reason"] == "compiler-artifact" and obj.get("target", {}).get("name") == "hgs":
                target, profile = obj["target"], obj.get("profile", {})
                if (target.get("kind") != ["bin"] or target.get("src_path") != str(workspace / "src/main.rs")
                        or profile.get("test") is not True
                        or obj.get("package_id") != "path+" + workspace.as_uri() + "#hgs@1.56.0"):
                    raise ValueError("hgs test artifact descriptor")
                candidates.append(obj)
    if finished != [True] or len(candidates) != 1:
        raise ValueError("missing/duplicate/unsuccessful terminal or hgs artifact")
    return candidates[0], counts


def copy_elf(record, evidence):
    path = Path(record["executable"])
    target = Path(os.environ["CARGO_TARGET_DIR"])
    if path.parent != target / "debug/deps" or re.fullmatch(r"hgs-[0-9a-f]{16}", path.name) is None:
        raise ValueError("exact emitted ELF path")
    for parent in (target, target / "debug", target / "debug/deps"):
        if not stat.S_ISDIR(parent.lstat().st_mode):
            raise ValueError("artifact parent kind")
    before = path.lstat()
    if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid()
            or not 64 <= before.st_size <= ELF_CAP):
        raise ValueError("ELF type/link/size")
    h = hashlib.sha256()
    total = 0
    def nofollow(name, flags):
        return os.open(name, flags | os.O_NOFOLLOW | os.O_CLOEXEC)
    def private_copy(name, flags):
        return os.open(name, flags | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with open(path, "rb", buffering=0, opener=nofollow) as source, open(evidence / "hgs-test.elf.partial", "xb", buffering=0, opener=private_copy) as dest:
        opened = os.fstat(source.fileno())
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise ValueError("ELF original open changed")
        header = source.read(64)
        if (len(header) != 64 or header[:6] != b"\x7fELF\x02\x01"
                or struct.unpack_from("<H", header, 18)[0] != 62
                or struct.unpack_from("<H", header, 16)[0] not in (2, 3)):
            raise ValueError("ELF64 little-endian x86_64")
        data = header
        while data:
            total += len(data)
            if total > ELF_CAP or dest.write(data) != len(data):
                raise ValueError("ELF copy capacity/short write")
            h.update(data)
            data = source.read(65536)
        os.fsync(dest.fileno())
    after = path.lstat()
    attrs = ("st_dev", "st_ino", "st_mode", "st_uid", "st_gid", "st_nlink", "st_size", "st_mtime_ns", "st_ctime_ns")
    if total != before.st_size or any(getattr(before, k) != getattr(after, k) for k in attrs):
        raise ValueError("ELF changed during copy")
    os.chmod(evidence / "hgs-test.elf.partial", 0o600)
    os.rename(evidence / "hgs-test.elf.partial", evidence / "hgs-test.elf")
    return dict(path=str(path), bytes=total, sha256=h.hexdigest(), target="x86_64-unknown-linux-gnu",
                cargo_record=record, original_links=before.st_nlink,
                invoked_by_helper_or_workflow=False, copied_mode="0600")


def collect(workspace, evidence):
    errors = []
    artifact = None
    counts = None
    equal = False
    try:
        snapshot(workspace, evidence, "after")
        equal = bounded(evidence / "source-before.json", JSON_CAP) == bounded(evidence / "source-after.json", JSON_CAP)
        if not equal:
            raise ValueError("source endpoints changed")
    except Exception as error:
        errors.append("source-final:" + type(error).__name__ + ":" + str(error)[:160])
    try:
        cap = json.loads(bounded(evidence / "capture.json", JSON_CAP))
        raw = evidence / "cargo-combined.raw"
        status = bounded(evidence / "pipeline-status.txt", 32)
        if (cap.get("complete") is not True or cap.get("unknown_tail") is not False
                or cap.get("eof") is not True or cap.get("overflow") is not False
                or cap.get("errors") != [] or status != b"0\t0\n"
                or not all(type(cap.get(k)) is int for k in ("drained", "written", "synced"))
                or cap["drained"] != cap["written"] or cap["written"] != cap["synced"]
                or raw.stat().st_size != cap["written"] or cap["written"] > RAW_CAP
                or hash_file(raw, RAW_CAP) != cap["sha256"]):
            raise ValueError("missing/failed/incomplete Cargo capture or pipeline")
        record, counts = cargo_record(evidence, workspace)
        if not equal:
            raise ValueError("source equality unavailable")
        artifact = copy_elf(record, evidence)
    except Exception as error:
        errors.append("compiler-artifact:" + type(error).__name__ + ":" + str(error)[:160])
    emit(evidence / "compiler-evidence.json", dict(schema=1, context=context(), source_endpoints_equal=equal,
         artifact=artifact, cargo_record_counts=counts, errors=errors,
         compiler_evidence_candidate=not errors and artifact is not None,
         record_admission=False, produced_artifact_invocations_in_workflow=False,
         whole_graph_death_qualified=False, production_absence_qualified=False,
         physical_stack_or_RSS_qualified=False, application_source_budget_qualified=False,
         whole_entry_qualified=False, current_or_drain_qualified=False,
         original_58431_bridge=False, B_release=False))
    return 0 if not errors and artifact is not None else 4


def retention(evidence):
    rows = []
    total = 0
    for name, cap in RETAINED.items():
        path = evidence / name
        try:
            s = path.lstat()
        except FileNotFoundError:
            continue
        if not stat.S_ISREG(s.st_mode) or s.st_uid != os.geteuid() or s.st_size > cap:
            raise ValueError("unavailable/oversize named retention file")
        total += s.st_size
        if total > 384 * 1024 * 1024 - JSON_CAP:
            raise ValueError("aggregate diagnostic retention capacity")
        rows.append(dict(name=name, bytes=s.st_size, sha256=hash_file(path, cap)))
    if not rows:
        raise ValueError("no retained diagnostic evidence")
    emit(evidence / "retention.json", dict(schema=1, context=context(), files=rows,
         known_file_bytes=total, aggregate_cap_bytes=384 * 1024 * 1024,
         final_upload_receipt_not_yet_available=True, record_admission=False))
    return 0


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in ("before", "capture", "collect", "retention"):
        raise ValueError("one fixed passive evidence operation required")
    evidence = Path(os.environ["EVIDENCE_DIR"])
    workspace = Path(os.environ["GITHUB_WORKSPACE"])
    context()
    if sys.argv[1] == "capture":
        return capture(evidence)
    if sys.argv[1] == "before":
        snapshot(workspace, evidence, "before")
        return 0
    if sys.argv[1] == "retention":
        return retention(evidence)
    return collect(workspace, evidence)


if __name__ == "__main__":
    sys.exit(main())
