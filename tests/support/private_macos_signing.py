"""Test-only private identity lifecycle. No owner key items or trust writes.

Native execution requires a separately reviewed opt-in fixture and Mac slot.
All evidence is mechanics (modelled identity boundary).
"""
import hashlib
import json
import os
from pathlib import Path
import pwd
import resource
import shutil
import subprocess

QUALIFICATION = "mechanics (modelled identity boundary)"
CN = "Developer ID Application: Zerus mechanics fixture (FAKETEST01)"  # Synthetic, never an Apple identity.
SYSTEM_PATH = "/usr/bin:/bin:/usr/sbin:/sbin"


def event(name, **data):
    print(json.dumps(dict(qualification=QUALIFICATION, event=name, **data)), flush=True)


def private_environment(home, temporary):
    env = {"PATH": SYSTEM_PATH, "HOME": str(home), "TMPDIR": str(temporary),
           "LANG": "en_US.UTF-8", "PYTHONDONTWRITEBYTECODE": "1", "PYTHONUTF8": "1",
           "ZERUS_TEST_PRIVATE_MACOS_SIGNING": "1"}
    env["CLANG_MODULE_CACHE_PATH"] = str(temporary / "clang-cache")
    for key, name in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                      ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache"),
                      ("XDG_RUNTIME_DIR", "runtime"), ("TMUX_TMPDIR", "tmux")]:
        path = temporary / name
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
        env[key] = str(path)
    return env


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_private_home(home):
    if (os.getuid() == 0 or os.getuid() != os.geteuid()
            or home == Path(pwd.getpwuid(os.getuid()).pw_dir).resolve()
            or home.stat().st_uid != os.geteuid() or home.stat().st_mode & 0o077):
        raise RuntimeError("an owned private HOME distinct from the account HOME is required")


class PrivateIdentity:
    def __init__(self, root, home, source):
        self.root, self.home = root.resolve(), home.resolve()
        require_private_home(self.home)
        if (not self.root.is_relative_to(self.home) or self.root.stat().st_uid != os.geteuid()
                or self.root.stat().st_mode & 0o077 or "/login.keychain" in str(self.root)):
            raise RuntimeError("a fresh private child prefix without login-keychain aliases is required")
        self.keychain, self.certificate = self.root / "fixture.keychain-db", self.root / "fixture.der"
        self.host = self.root / "fixture-host"
        self.source = source
        self.env = private_environment(self.home, self.root / "tmp")
        self.baseline = None
        self.created = False
        self.signing_checked = False
        self.uncertain = False
        self.sha1 = None
        self.calls = []

    def public_command(self, argv, *, input_data=None):
        # All callers supply explicit public argv. The only secret input here is
        # a one-time PEM import pipe; the C host never echoes its input.
        try:
            result = subprocess.run(list(map(str, argv)), input=input_data, env=self.env,
                                    capture_output=True, timeout=60)
        except (OSError, subprocess.SubprocessError):
            self.uncertain = True
            event("owned_tool_unknown", argv=[str(x) for x in argv], retained=str(self.root))
            raise
        self.calls.append([str(x) for x in argv])
        event("tool", argv=[str(x) for x in argv], exit=result.returncode,
              stdout=result.stdout.decode("utf-8", "replace"), stderr=result.stderr.decode("utf-8", "replace"))
        if result.returncode:
            raise RuntimeError(f"{QUALIFICATION}: owned tool failed (exit {result.returncode})")
        return result.stdout

    def references(self, phase):
        # Coordinator-approved metadata reads only. No identity/item queries on
        # owner keychains, and no setters. Compare raw responses in RAM while
        # recording only home-redacted references and their hashes.
        commands = [["/usr/bin/security", "list-keychains", "-d", domain]
                    for domain in ("user", "common", "dynamic")]
        commands += [["/usr/bin/security", "default-keychain", "-d", "user"],
                     ["/usr/bin/security", "login-keychain"]]
        actual_home = pwd.getpwuid(os.getuid()).pw_dir
        snapshot = []
        for command in commands:
            try:
                result = subprocess.run(command, env=self.env, capture_output=True, timeout=20)
            except (OSError, subprocess.SubprocessError):
                self.uncertain = True
                raise
            if len(result.stdout) + len(result.stderr) > 65536:
                self.uncertain = True
                raise RuntimeError("keychain reference metadata exceeds the fixture bound")
            if b"interaction" in result.stderr.lower():
                self.uncertain = True
                raise RuntimeError("keychain reference metadata requested interaction; stop")
            snapshot.append((result.returncode, result.stdout, result.stderr))
            event("keychain_references", phase=phase, argv=command, exit=result.returncode,
                  stdout=result.stdout.decode("utf-8", "replace").replace(actual_home, "~"),
                  stderr=result.stderr.decode("utf-8", "replace").replace(actual_home, "~"),
                  response_sha256=hashlib.sha256(result.stdout + b"\0" + result.stderr).hexdigest())
        return snapshot

    def preference_files(self):
        directory = self.home / "Library/Preferences"
        if not directory.exists():
            return {}
        files = sorted(directory.rglob("*"))
        if len(files) > 64:
            raise RuntimeError("private preference fixture exceeds its file bound")
        result = {}
        for path in files:
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise RuntimeError("unsafe private preference entry")
            if path.is_file():
                if path.stat().st_size > 1024 * 1024:
                    raise RuntimeError("private preference file exceeds its byte bound")
                result[str(path.relative_to(directory))] = sha256(path)
        return result

    def unchanged_references(self, phase):
        current = self.references(phase)
        if current != self.baseline:
            self.uncertain = True
            event("scope_changed", phase=phase, retained=str(self.root))
            raise RuntimeError("keychain references changed; stop without state repair or codesign")
        event("keychain_references_unchanged", phase=phase)

    def create(self):
        require_private_home(self.home)
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        self.public_command(["/usr/bin/xcrun", "clang", "-Wno-deprecated-declarations", "-g0",
                             str(self.source), "-framework", "Security", "-framework", "CoreFoundation",
                             "-o", str(self.host)])
        # OpenSSL emits the private key only to this process's captured RAM. No
        # stdout/stderr from this command enters evidence, including on failure.
        configuration = ("[req]\nprompt=no\ndistinguished_name=dn\nx509_extensions=code\n"
                         "[dn]\nCN=" + CN + "\nOU=FAKETEST01\nO=Zerus synthetic fixture\n"
                         "[code]\nbasicConstraints=critical,CA:FALSE\n"
                         "keyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\n").encode()
        generated = subprocess.run(["/usr/bin/openssl", "req", "-new", "-x509", "-newkey", "rsa:2048",
                                    "-nodes", "-days", "1", "-config", "/dev/stdin", "-keyout", "/dev/stdout",
                                    "-out", "/dev/stdout"], input=configuration, env=self.env,
                                   capture_output=True, timeout=60)
        if generated.returncode or len(generated.stdout) > 65535:
            raise RuntimeError("synthetic in-memory identity generation failed; secret output withheld")
        self.baseline = self.references("before_creation")
        self.preferences_before = self.preference_files()
        try:
            output = self.public_command([self.host, "create", self.keychain, self.certificate],
                                         input_data=generated.stdout)
        finally:
            generated = None  # Only RAM held the plaintext key; never log it.
            self.unchanged_references("after_creation")
        proof = json.loads(output)
        if not proof.get("private_identity_proved"):
            raise RuntimeError("private identity key/certificate proof missing")
        self.sha1 = proof["sha1"]
        if hashlib.sha1(self.certificate.read_bytes()).hexdigest().upper() != self.sha1:
            raise RuntimeError("private certificate proof mismatch")
        self.created = True
        event("private_identity", keychain=str(self.keychain), certificate=str(self.certificate),
              certificate_sha256=sha256(self.certificate), private_identity_proved=True)
        return self

    def after_signing(self):
        self.unchanged_references("after_codesign")
        self.signing_checked = True

    def cleanup(self):
        if self.uncertain:
            event("cleanup_uncertain_retained", root=str(self.root))
            raise RuntimeError("uncertain scope retained; no owner-state repair")
        if self.baseline is not None:
            if self.created and not self.signing_checked:
                self.unchanged_references("after_codesign_abort")
            self.unchanged_references("before_deletion")
        if self.keychain.exists():
            event("owned_keychain_before_delete", keychain=str(self.keychain), sha256=sha256(self.keychain))
            self.public_command([self.host, "delete", self.keychain])
        if self.keychain.exists():
            raise RuntimeError("private keychain deletion unconfirmed")
        if self.baseline is not None:
            self.unchanged_references("after_deletion")
            if self.preference_files() != self.preferences_before:
                self.uncertain = True
                raise RuntimeError("private preference effects changed; retained for review")
        if self.certificate.exists():
            event("owned_certificate_before_delete", certificate=str(self.certificate), sha256=sha256(self.certificate))
            self.certificate.unlink()
        event("private_identity_deleted", keychain_exists=self.keychain.exists(),
              certificate_exists=self.certificate.exists(),
              private_preferences_unchanged=True if self.baseline is not None else None)
        shutil.rmtree(self.root)
        event("scratch_cleanup", exists=self.root.exists())
