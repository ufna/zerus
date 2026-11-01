#!/usr/bin/env python3
"""Run the fixed policy controls without changing Windows execution policy."""
import base64
from pathlib import Path
import subprocess


root = Path(__file__).resolve().parents[2]
test = root / "scripts/ci/windows-desktop-registration-tests.ps1"
policy = root / "src/desktop/windows_registration.ps1"
source = test.read_text(encoding="utf-8")
assert len(source.encode("utf-8")) <= 16 * 1024
argument = str(policy).replace("'", "''")
command = "& {\n" + source + "\n} -RegistrationSource '" + argument + "'"
encoded = base64.b64encode(command.encode("utf-16le")).decode("ascii")
assert len(encoded) < 30_000
subprocess.run(
    ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
    check=True,
    timeout=60,
)
