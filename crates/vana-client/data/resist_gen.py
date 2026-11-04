"""Writes data/resistances.txt from LandSandBoat's data/ (YAML): the monsters' elemental resistance ranks.

usage: python3 -I resist_gen.py LSB_DIR OUT

The ranks are rank_element as LSB merges it (dataset.cpp: ecosystem, then family, then species, then the monster's
template, each overriding single elements). Higher resists more; below 0 is a weakness.
"""
import sys
from pathlib import Path

import yaml

ELEMENTS = ["fire", "ice", "wind", "earth", "thunder", "water", "light", "dark"]


def ranks(attrs):
    """The rank_element values a layer sets, by element."""
    r = ((attrs or {}).get("resists") or {}).get("rank_element") or {}
    return {e: int(v) for e, v in r.items() if e in ELEMENTS and v is not None}


def main(lsb, out):
    data = Path(lsb) / "data"
    eco = yaml.safe_load((data / "ecosystems.yaml").read_text())["ecosystems"]
    species = {}
    for e in eco.values():
        base = ranks(e.get("attributes"))
        for f in (e.get("families") or {}).values():
            fam = {**base, **ranks(f.get("attributes"))}
            for name, s in (f.get("species") or {}).items():
                species[name] = {**fam, **ranks(s.get("attributes"))}
    zones = yaml.safe_load((data / "enums" / "zone.yaml").read_text())
    zone_ids = next(v for v in zones.values() if isinstance(v, dict) and "west_ronfaure" in v)
    lines = [
        "# vana-client data/resist_gen.py from LandSandBoat data/: ecosystems.yaml, zones/*/mobs.yaml (rank_element)",
        "# s species fire ice wind earth thunder water light dark / m zone name species [element=rank...]",
    ]
    for name in sorted(species):
        lines.append("s %s %s" % (name, " ".join(str(species[name].get(e, 0)) for e in ELEMENTS)))
    seen = set()
    for path in sorted((data / "zones").glob("*/mobs.yaml")):
        zone = zone_ids.get(path.parent.name)
        doc = yaml.safe_load(path.read_text()) or {}
        if zone is None:
            continue
        for key, t in (doc.get("templates") or {}).items():
            if not isinstance(t, dict) or t.get("species") not in species:
                continue
            shown = str(t.get("display_name") or key).replace(" ", "_")
            if (zone, shown) in seen:
                continue
            seen.add((zone, shown))
            own = ranks(t.get("attributes"))
            extra = "".join(" %s=%d" % (e, own[e]) for e in ELEMENTS if e in own)
            lines.append("m %d %s %s%s" % (zone, shown, t["species"], extra))
    Path(out).write_text("\n".join(lines) + "\n")
    print("wrote %s: %d species, %d monsters" % (out, len(species), len(seen)))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
