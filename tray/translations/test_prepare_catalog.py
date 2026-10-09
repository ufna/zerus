import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from prepare_catalog import prepare


class CatalogTests(unittest.TestCase):
    def run_catalog(self, entries):
        with tempfile.TemporaryDirectory() as folder:
            source, output = Path(folder) / "source.ts", Path(folder) / "output.ts"
            root = ET.Element("TS", language="ru_RU")
            for context_name, text, translated, plural in entries:
                context = ET.SubElement(root, "context")
                ET.SubElement(context, "name").text = context_name
                message = ET.SubElement(context, "message", {"numerus": "yes"} if plural else {})
                ET.SubElement(message, "source").text = text
                translation = ET.SubElement(message, "translation")
                if plural:
                    for form in translated:
                        ET.SubElement(translation, "numerusform").text = form
                else:
                    translation.text = translated
            ET.ElementTree(root).write(source, encoding="utf-8")
            prepare(source, output)
            return ET.parse(output).getroot()

    def test_rejects_changed_placeholders_and_markup(self):
        for translated in ["Сохранено", "<b>%2</b>", "<i>%1</i>"]:
            with self.subTest(translated=translated), self.assertRaises(ValueError):
                self.run_catalog([("Widget", "<b>%1</b>", translated, False)])

    def test_requires_all_russian_plural_forms(self):
        with self.assertRaises(ValueError):
            self.run_catalog([("Widget", "%n items", ["%n элемент"], True)])

    def test_does_not_guess_ambiguous_context_translations(self):
        root = self.run_catalog([
            ("One", "Open", "Открыть", False),
            ("Two", "Open", "Открыто", False),
            ("One", "Settings", "Настройки", False),
            ("Two", "Settings", "Настройки", False),
        ])
        shared = root.find("context[name='Zerus']")
        sources = [message.findtext("source") for message in shared.findall("message")]
        self.assertEqual(sources, ["Settings"])
        self.assertEqual(len(root.findall("context[name='One']/message")), 2)


if __name__ == "__main__":
    unittest.main()
