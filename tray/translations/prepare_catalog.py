#!/usr/bin/env python3
"""Validate a Qt catalog and add a build-only context for inherited tr() calls."""

import argparse
import copy
import re
import xml.etree.ElementTree as ET
from collections import defaultdict


def prepare(source, destination):
    tree = ET.parse(source)
    root = tree.getroot()
    candidates = defaultdict(list)
    tokens = re.compile(r"%L?\d+|%n|<[^>]+>")
    for message in root.findall(".//message"):
        text = message.findtext("source", "")
        translation = message.find("translation")
        if translation is None or translation.get("type") in {"unfinished", "obsolete", "vanished"}:
            raise ValueError(f"Missing translation: {text!r}")
        forms = translation.findall("numerusform")
        values = [form.text or "" for form in forms] if forms else [translation.text or ""]
        if message.get("numerus") == "yes" and root.get("language", "").startswith("ru") and len(forms) != 3:
            raise ValueError(f"Russian plurals need three forms: {text!r}")
        for value in values:
            if not value or sorted(tokens.findall(text)) != sorted(tokens.findall(value)):
                raise ValueError(f"Empty translation or changed placeholders/markup: {text!r}")
        if not message.findtext("comment"):
            candidates[text].append(message)

    # Preserve native context/disambiguation lookup. Use a shared fallback only
    # where a source has an unambiguous translation across the catalog.
    shared = next((context for context in root.findall("context") if context.findtext("name") == "Zerus"), None)
    if shared is None:
        shared = ET.SubElement(root, "context")
        ET.SubElement(shared, "name").text = "Zerus"
    present = {message.findtext("source") for message in shared.findall("message")}
    for text, messages in candidates.items():
        variants = {ET.tostring(message.find("translation"), encoding="unicode").strip() for message in messages}
        if text in present or len(variants) != 1:
            continue
        message = copy.deepcopy(messages[0])
        for location in message.findall("location"):
            message.remove(location)
        shared.append(message)
    ET.indent(root)
    tree.write(destination, encoding="utf-8", xml_declaration=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("destination")
    args = parser.parse_args()
    prepare(args.source, args.destination)
