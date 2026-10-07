"""Build a self-contained, offline HTML preview; does not modify icon artwork."""
import base64
import json
from pathlib import Path

root = Path(__file__).resolve().parent
variants = [
    dict(id="01-rune", name="Rune", label="Руна", style="Знак · плоская графика",
         description="Буква Z, собранная из двух хитиновых лезвий. Ближе всего к текущей идее, с более простым силуэтом.",
         character="Резкий, собранный, узнаваемый", accent="#ad8aff"),
    dict(id="02-swarm", name="Swarm", label="Рой", style="Символ · вращение",
         description="Три органических крыла вокруг открытого центра. Отсылка к рою и нескольким агентам, работающим вместе.",
         character="Живой, динамичный, коллективный", accent="#76dcc0"),
    dict(id="03-seed", name="Seed", label="Семя", style="Объект · мягкий объём",
         description="Первобытное семя из трёх оболочек. Лиловый панцирь и тёплое ядро — спокойная, более предметная иконка.",
         character="Тактильный, спокойный, органический", accent="#e6bb87"),
    dict(id="04-sentinel", name="Sentinel", label="Страж", style="Персонаж · маска",
         description="Лицо инопланетного стража с хитиновым гребнем. Самая буквальная отсылка к зергам, с характером маскота.",
         character="Выразительный, инопланетный, личный", accent="#c196ed"),
]
for variant in variants:
    png = root / "assets" / (variant["id"] + "-512.png")
    variant["image"] = "data:image/png;base64," + base64.b64encode(png.read_bytes()).decode("ascii")
payload = {"variants": variants, "prompts": json.loads((root / "prompts.json").read_text())}
data = json.dumps(payload, ensure_ascii=False).replace("<", "\\u003c")
template = (root / "preview.template.html").read_text()
assert template.count("__ZERUS_DATA__") == 1
(root / "index.html").write_text(template.replace("__ZERUS_DATA__", data))
print("Built offline preview with 4 embedded icon variants.")
