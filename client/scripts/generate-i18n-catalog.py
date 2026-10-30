#!/usr/bin/env python3
"""Generate Next's source-keyed Russian catalog from the reviewed Qt catalog."""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "tray/translations/zerus_ru.ts"
DESTINATION = ROOT / "client/src/i18n.ru.json"


def generate() -> str:
    spec = importlib.util.spec_from_file_location("zerus_prepare_catalog", ROOT / "tray/translations/prepare_catalog.py")
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    with tempfile.TemporaryDirectory(prefix="zerus-next-i18n-") as scratch:
        prepared = Path(scratch) / "zerus_ru.ts"
        module.prepare(SOURCE, prepared)
        root = module.read_catalog(prepared)
    shared = next((context for context in root.findall("context") if context.findtext("name") == "Zerus"), None)
    if shared is None:
        raise ValueError("The prepared catalog has no unambiguous Zerus context")
    catalog = {}
    for message in shared.findall("message"):
        source = message.findtext("source", "")
        translation = message.find("translation")
        if not source or translation is None:
            continue
        forms = translation.findall("numerusform")
        value = [form.text or "" for form in forms] if forms else translation.text or ""
        values = value if isinstance(value, list) else [value]
        if "·" in source or any("·" in item for item in values):
            continue
        if source in catalog and catalog[source] != value:
            raise ValueError(f"Conflicting shared translation: {source!r}")
        catalog[source] = value
    return json.dumps(catalog, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail unless the checked-in catalog is current")
    args = parser.parse_args()
    generated = generate()
    if args.check:
        if not DESTINATION.exists() or DESTINATION.read_text(encoding="utf-8") != generated:
            raise SystemExit(f"Run {Path(__file__).relative_to(ROOT)} to refresh {DESTINATION.relative_to(ROOT)}")
        print(f"Next Russian catalog is current: {DESTINATION.relative_to(ROOT)}")
    else:
        DESTINATION.write_text(generated, encoding="utf-8")
        print(f"Wrote {DESTINATION.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
