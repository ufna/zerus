"""Build the second, standalone selection preview without editing image pixels."""
import base64
import json
from pathlib import Path

root = Path(__file__).resolve().parent
variants = [
    dict(id="01-rune-turquoise-v2", name="Rune", label="Руна · 01", style="Бирюзовый объём · монограмма",
         description="Та же текучая буква Z: округлые грани, приподнятая диагональ и бирюзовый материал. Глубина видна в крупной иконке, силуэт узнаётся в трее.",
         character="Точный знак с прямой связью с именем Zerus", accent="#77dfd0", silhouette="01-rune-512.png"),
    dict(id="02-swarm-turquoise-v2", name="Swarm", label="Рой · 02", style="Бирюзовый объём · вращение",
         description="Три объёмных крыла и утопленные внутренние лезвия. Единая бирюзовая палитра сохраняет вращение и ощущение живого роя.",
         character="Органический символ нескольких агентов", accent="#77dfd0", silhouette="02-swarm-512.png"),
]
def image(name):
    return "data:image/png;base64," + base64.b64encode((root / "assets" / name).read_bytes()).decode("ascii")

for variant in variants:
    variant["image"] = image(variant["id"] + "-1024.png")
    variant["silhouette"] = image(variant["silhouette"])
payload = {"variants": variants, "prompts": json.loads((root / "prompts-turquoise-v2.json").read_text())}
data = json.dumps(payload, ensure_ascii=False).replace("<", "\\u003c")
template = (root / "turquoise.template.html").read_text()
assert template.count("__ZERUS_DATA__") == 1
(root / "turquoise-v2.html").write_text(template.replace("__ZERUS_DATA__", data))
print("Built turquoise preview: two sculpted app icons and their flat tray silhouettes.")
