"""Build the fourth offline preview: Swarm material studies on neutral graphite."""
import base64
import json
from pathlib import Path

root = Path(__file__).resolve().parent
variants = [
    dict(id="swarm-inlay-v4", name="Inlay", label="Инкрустация", style="Титан · эмаль · янтарь",
         description="Металлическая кромка, утопленная бирюзовая эмаль и янтарные вставки. Ступенчатые грани и разные материалы создают объём.",
         character="Точная оправа и выразительная инкрустация", accent="#cfba95",
         materials=[dict(name="Титан", color="#a9aaa7"), dict(name="Эмаль", color="#138b85"), dict(name="Янтарь", color="#e5a541")]),
    dict(id="swarm-carapace-v4", name="Carapace", label="Панцирь", style="Матовый хитин · минерал",
         description="Слоистый тёмный панцирь, острые срезы и глубокие бирюзовые полости. Коралловые вставки выглядят как части живого минерала.",
         character="Ближе всего к первобытному Зерусу", accent="#daa28e",
         materials=[dict(name="Хитин", color="#575b5f"), dict(name="Нефрит", color="#438f85"), dict(name="Коралл", color="#d97954")]),
    dict(id="swarm-porcelain-v4", name="Porcelain", label="Керамика", style="Керамика · эмаль · медь",
         description="Светлые керамические грани, бирюзовая инкрустация и медные лезвия. Более светлая композиция с чётким разделением поверхностей.",
         character="Светлая керамика и архитектурная пластика", accent="#dfd5be",
         materials=[dict(name="Керамика", color="#e9e1d0"), dict(name="Эмаль", color="#209b9e"), dict(name="Медь", color="#c17d49")]),
]
def image(name):
    return "data:image/png;base64," + base64.b64encode((root / "assets" / name).read_bytes()).decode("ascii")

silhouette = image("02-swarm-512.png")
for variant in variants:
    variant["image"] = image(variant["id"] + "-1024.png")
    variant["silhouette"] = silhouette
payload = {
    "variants": variants,
    "baseline": {"id": "swarm-ember-v3", "name": "Предыдущая версия", "image": image("swarm-ember-v3-1024.png"), "silhouette": silhouette},
    "prompts": json.loads((root / "prompts-swarm-materials-v4.json").read_text()),
}
data = json.dumps(payload, ensure_ascii=False).replace("<", "\\u003c")
template = (root / "swarm-materials.template.html").read_text()
assert template.count("__ZERUS_DATA__") == 1
(root / "swarm-materials-v4.html").write_text(template.replace("__ZERUS_DATA__", data))
print("Built material preview: Inlay, Carapace and Porcelain, with previous-version comparison.")
