"""Build a self-contained comparison of the selected Swarm and previous Z mark."""
import json
from pathlib import Path

root = Path(__file__).resolve().parent
variants = [
    dict(id="swarm-symbolic-v6", name="Swarm", subtitle="Лезвия", tag="Выбранный вариант",
         description="Три изогнутых лезвия, собранных в вихрь. Чистый силуэт и открытый центр.",
         svg=(root / "assets/swarm-symbolic-v6.svg").read_text()),
    dict(id="z-carapace-symbolic", name="Carapace Z", subtitle="Буква Z", tag="Предыдущий вариант",
         description="Знакомая Z с заострёнными краями. Предыдущий плоский знак из трея.",
         svg=(root / "assets/z-carapace-symbolic-v5.svg").read_text()),
]
payload = json.dumps(variants, ensure_ascii=False).replace("<", "\\u003c")
template = (root / "monochrome.template.html").read_text()
assert template.count("__ZERUS_DATA__") == 1
(root / "monochrome-v6.html").write_text(template.replace("__ZERUS_DATA__", payload))
print("Built monochrome-v6.html: selected Swarm blades and previous Carapace Z.")
