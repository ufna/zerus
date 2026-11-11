#!/usr/bin/env python3
"""Mutate production guards in a scratch copy; never exercise a real session.

Uses this worktree's compiled Cargo dependencies. Policy, engine and real
SQLite ledger tests run unchanged. Native pure record/ownership functions are
extracted unchanged; full hgs tests cover their I/O adapters and audit storage.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
SOURCE = Path(__file__).resolve().parent
DEPS = ROOT / "target/debug/deps"


def block(text, start):
    begin = text.index("{", start)
    depth = 1
    end = begin + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


def native_pure(directory):
    observation = (SOURCE / "native/observation.rs").read_text()
    tests = observation[observation.index("#[cfg(test)]"):]
    pure = "\n".join(block(observation, observation.index(name)) for name in
                     ["fn text(", "pub(super) fn parse_view(", "pub(super) fn paused(", "pub(super) fn self_exit("])
    (directory / "native/observation.rs").write_text("use super::*;\n" + pure + "\n" + tests)
    original = (SOURCE / "native/original.rs").read_text()
    test = block(original, original.index("fn a_final_record_read"))
    pure = block(original, original.index("fn unchanged_record("))
    (directory / "native/original.rs").write_text("use super::*;\n" + pure + "\n#[test]\n" + test)
    owner = (SOURCE / "native/ownership.rs").read_text()
    owner = owner.replace(block(owner, owner.index("impl Native<'_>")), "")
    (directory / "native/ownership.rs").write_text(owner)


def harness(directory):
    modules = "\n".join(f"#[path={json.dumps(str(directory / (m + '.rs')))}] mod {m};" for m in
                        ["types", "ports", "policy", "engine", "ledger", "resumed"])
    native = "\n".join(f"#[path={json.dumps(str(directory / 'native' / (m + '.rs')))}] mod {m};" for m in
                       ["observation", "original", "ownership"])
    return r'''#![allow(dead_code, unused_imports)]
mod platform {
    pub enum Private { File }
    pub fn canonical(p: impl AsRef<std::path::Path>)->std::io::Result<std::path::PathBuf> { std::fs::canonicalize(p) }
    pub fn verify_private(p: &std::path::Path,_:Private)->std::io::Result<bool> {Ok(std::fs::metadata(p)?.is_file())}
}
mod approval_log {
    pub fn open(root:&std::path::Path)->Result<rusqlite::Connection,String> {
        std::fs::create_dir_all(root).map_err(|e|e.to_string())?;
        let db=rusqlite::Connection::open(root.join("journal.sqlite3")).map_err(|e|e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL").map_err(|e|e.to_string())?;
        Ok(db)
    }
}
mod teams {pub mod ops {pub type Request=serde_json::Value;}}
mod desktop {pub mod teams {pub mod confirm {
''' + modules + "\nmod native { use super::{types::*,ports::Result}; use serde_json::{json,Value};\n" + native + "\n}\n}}}\n"


def commands(scratch):
    command = ["rustup", "run", "1.99.1", "rustc", "--edition=2021", "--test", str(scratch / "harness.rs"),
               "-L", "dependency=" + str(DEPS), "-o", str(scratch / "tests")]
    fingerprint = ROOT / "target/debug/.fingerprint"
    targets = [p for p in fingerprint.glob("hgs-*/test-bin-hgs.json") if (DEPS / p.parent.name).is_file()]
    target = max(targets, key=os.path.getmtime)
    wanted = {dep[1]: dep[3] for dep in json.loads(target.read_text())["deps"]}
    for name in ["serde", "serde_json", "sha2", "uuid", "zerus_team_plan", "rusqlite", "tempfile", "libc"]:
        matched = [p for p in fingerprint.glob("*/lib-" + name)
                   if int.from_bytes(bytes.fromhex(p.read_text()), "little") == wanted[name]]
        if len(matched) != 1:
            raise RuntimeError("Cannot resolve Cargo dependency: " + name)
        suffix = matched[0].parent.name.rsplit("-", 1)[1]
        command += ["--extern", name + "=" + str(DEPS / ("lib" + name + "-" + suffix + ".rlib"))]
    return command


def need_mutants(source):
    for index, match in enumerate(re.finditer(r"\bneed\(", source)):
        if source[max(0, match.start() - 4):match.start()] == "fn ":
            continue
        start = match.start()
        depth, end = 1, match.end()
        while depth:
            depth += (source[end] == "(") - (source[end] == ")")
            end += 1
        if source[end:end + 1] != "?":
            continue
        line = source[:start].count("\n") + 1
        yield f"policy_need_{index}_line_{line}", "policy.rs", source[start:end], "Ok::<(),String>(())"


def mutations(directory):
    policy = (directory / "policy.rs").read_text()
    yield from need_mutants(policy)
    yield "claim_and_audit_replay", "engine.rs", "if !claim.fresh {", "if false {"
    yield "recovered_claim_uncertain", "engine.rs", 'row.state = "unknown".into();', 'row.state = "cancelled".into();'
    yield "journal_missing_before_delivery", "engine.rs", "p.audit_begin(&row.plan)", 'Ok(Audit {id:1,request:uuid::Uuid::new_v4().to_string(),answers:json!([]),fresh:true})'
    yield "audit_binding_not_saved_before_delivery", "engine.rs", "p.save(row)?;\n    if !row.audit", "if !row.audit"
    yield "finish_journal", "engine.rs", "p.audit_finish(row).err()", "None"
    yield "recover_orphan_audit", "engine.rs", "recover_audit(p, &mut row).err()", "None"
    yield "stable_card_after_restart", "engine.rs", "p.card(row)?;", "row.card_request=uuid::Uuid::new_v4().to_string(); p.card(row)?;"
    yield "question_hash_reopens_original", "engine.rs", 'b.launch, s.run, tag', 'b.launch, s.run, tag, question["question_hash"]'
    yield "exit_metadata_reopens_original", "engine.rs", 'b.launch, s.run, tag', 'b.launch, s.run, tag, s.exit_identity'
    yield "fourth_resume_allowed", "ledger.rs", "Ok(count >= 3)", "Ok(count >= 4)"
    yield "quota_forgets_previous_epoch", "ledger.rs", "now.saturating_sub(3_600_000)", "now"
    yield "terminal_state_reopened", "ledger.rs", '(expected != "prepared" && expected != row.state)', "false"
    yield "original_identity_replaced", "ledger.rs", "old.original != row.original", "false"
    yield "plan_identity_replaced", "ledger.rs", "old.plan != row.plan", "false"
    yield "uncertain_binding_published", "ledger.rs", 'old.state == "approved" && old.plan.kind == Kind::Resume', 'old.plan.kind == Kind::Resume'
    yield "native_view_owner_ignored", "native/observation.rs", 'raw["worktree_owner"]["id"] != b.nonce', "false"
    yield "supervisor_origin_ignored", "native/observation.rs", 'v["completion_source"] != "supervisor"', "false"
    yield "ambiguous_exit_allowed", "native/observation.rs", "_ => false,", "_ => true,"
    yield "archived_crash_restored", "native/observation.rs", "if !archived", "if true"
    yield "final_native_record_replaced", "native/original.rs", "current[key] != raw[key]", "false"
    yield "ownership_replacement_ignored", "native/ownership.rs", "version(&m) != self.version", "false"


def execute(command, log):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=120)
    log.write("COMMAND: " + " ".join(command) + "\n" + result.stdout + f"EXIT: {result.returncode}\n")
    log.flush()
    return result.returncode


def check_one(directory, command, log, mutation, baseline):
    name, file, old, new = mutation
    original = baseline[file]
    if old not in original:
        raise RuntimeError(f"Mutant target missing: {name}: {old}")
    changed = original.replace(old, new, 1)
    if name == "ownership_replacement_ignored":
        if "version(&m) == self.version" not in changed:
            raise RuntimeError("Held ownership version target missing")
        changed = changed.replace("version(&m) == self.version", "true")
    if name == "claim_and_audit_replay":
        changed = changed.replace("if !row.audit.as_ref().is_some_and(|audit| audit.fresh) {", "if false {")
    (directory / file).write_text(changed)
    log.write("\nMUTANT: " + name + "\n")
    if execute(command, log):
        raise RuntimeError("Mutant did not compile: " + name)
    killed = execute([command[command.index("-o") + 1], "--test-threads=8"], log) != 0
    (directory / file).write_text(original)
    print(("KILLED " if killed else "SURVIVED ") + name, flush=True)
    return killed


def main():
    output = ROOT / "artifacts/test-results/wp17-mutants.log"
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="zerus-confirm-mutants-") as temporary, output.open("w") as log:
        scratch = Path(temporary)
        directory = scratch / "src/desktop/teams/confirm"
        shutil.copytree(SOURCE, directory)
        fixture = scratch / "crates/team-plan/fixtures"
        fixture.mkdir(parents=True)
        shutil.copy(ROOT / "crates/team-plan/fixtures/team-queue.json", fixture)
        for module in ["policy", "engine", "ledger"]:
            path = directory / (module + ".rs")
            text = path.read_text()
            text = text.replace("pub(super) mod tests;", '#[path="' + module + '/tests.rs"] pub(super) mod tests;') if module == "policy" else text.replace("mod tests;", '#[path="' + module + '/tests.rs"] mod tests;')
            path.write_text(text)
        native_pure(directory)
        (scratch / "harness.rs").write_text(harness(directory))
        command = commands(scratch)
        if execute(command, log) or execute([str(scratch / "tests"), "--test-threads=8"], log):
            raise RuntimeError("Baseline failed; see " + str(output))
        baseline = {str(p.relative_to(directory)): p.read_text() for p in directory.rglob("*.rs")}
        cases = list(mutations(directory))
        killed = sum(check_one(directory, command, log, case, baseline) for case in cases)
        result = f"MUTANTS: {killed} killed; {len(cases) - killed} survived; {len(cases)} executed"
        print(result)
        log.write(result + "\n")
        if killed != len(cases):
            raise SystemExit(1)


if __name__ == "__main__":
    main()
