"""Build the third offline preview: four color pairs for the same Swarm shape."""
import base64
import json
from pathlib import Path

root = Path(__file__).resolve().parent
variants = [
    dict(id="swarm-ember-v3", name="Ember", label="Коралл", style="Бирюза × коралл",
         description="Холодная бирюза и горячие коралловые вставки. Самое резкое разделение двух групп лезвий по температуре цвета.",
         character="Самый выразительный тёпло-холодный контраст", accent="#ff927d", inner="#ff6552"),
    dict(id="swarm-solar-v3", name="Solar", label="Янтарь", style="Бирюза × янтарь",
         description="Яркие янтарные вставки добавляют тепла. Внутренний круг заметен сразу, а весь знак выглядит чуть мягче кораллового.",
         character="Тёплый акцент с ярким золотистым объёмом", accent="#ffd274", inner="#ffbe3e"),
    dict(id="swarm-ion-v3", name="Ion", label="Лайм", style="Бирюза × лайм",
         description="Жёлто-лаймовые вставки создают ощущение живой энергии. Самая яркая и биологическая пара в этой подборке.",
         character="Яркий биологический характер роя", accent="#d5f542", inner="#d5f542"),
    dict(id="swarm-pearl-v3", name="Pearl", label="Жемчуг", style="Бирюза × жемчуг",
         description="Светлые жемчужные вставки выделяются за счёт яркости. Спокойная пара, в которой хорошо читается форма каждой детали.",
         character="Световой контраст и сдержанная палитра", accent="#dcece7", inner="#f1f6ed"),
]
def image(name):
    return "data:image/png;base64," + base64.b64encode((root / "assets" / name).read_bytes()).decode("ascii")

silhouette = image("02-swarm-512.png")
for variant in variants:
    variant["image"] = image(variant["id"] + "-1024.png")
    variant["silhouette"] = silhouette
    variant["outer"] = "#20c7bc"
payload = {"variants": variants, "prompts": json.loads((root / "prompts-swarm-colors-v3.json").read_text())}
data = json.dumps(payload, ensure_ascii=False).replace("<", "\\u003c")
template = (root / "swarm-colors.template.html").read_text()
assert template.count("__ZERUS_DATA__") == 1
(root / "swarm-colors-v3.html").write_text(template.replace("__ZERUS_DATA__", data))
print("Built Swarm color preview: coral, amber, lime and pearl.")
