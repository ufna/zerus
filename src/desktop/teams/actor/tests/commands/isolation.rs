//! Check the actual subprocess environment before any tmux client can run.
use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn every_fixture_child_has_an_existing_private_tmux_directory() {
    let h = Harness::new();
    let mut config = super::super::production::native(&h).config;
    config.home = h.temp.path().display().to_string();
    let program = h.temp.path().join("check-tmux-environment");
    private_write(
        &program,
        br#"#!/usr/bin/env python3
import os, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1])
# Fail before running a client if the fixture could reach the default or an
# inherited server. Even a mutant cannot query the real socket in this test.
assert os.environ.get('TMUX_TMPDIR') == str(root), 'unisolated tmux child'
assert 'TMUX' not in os.environ, 'inherited tmux socket'
assert root.is_dir() and root.stat().st_mode & 0o777 == 0o700
result = subprocess.run(['tmux', 'list-panes', '-a'], capture_output=True, text=True, timeout=5)
assert result.returncode != 0, 'a child saw an existing tmux server'
assert str(root) in result.stderr, result.stderr
"#,
    );
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let expected = h.temp.path().join("tmux");
    let f = Fixture {
        h,
        config,
        cli: program,
        task: task(1),
        node: 1,
        id: String::new(),
    };
    let result = f.process(&[expected.display().to_string()], "", None);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
