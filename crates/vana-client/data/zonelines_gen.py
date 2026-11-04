"""Writes data/zonelines.txt from LandSandBoat's data/ (YAML): where each zone line leads.

usage: python3 -I zonelines_gen.py LSB_DIR OUT

Each zone's zone.yaml keys its zone lines by the four-character token the client sends in 0x05E (the region's first
four letters); `to` names the destination in data/enums/zone.yaml. Lines: from-zone token to-zone.
"""
import sys
from pathlib import Path

import yaml


def main(lsb, out):
    data = Path(lsb) / "data"
    ids = yaml.safe_load((data / "enums" / "zone.yaml").read_text())["values"]
    rows = []
    for f in sorted((data / "zones").glob("*/zone.yaml")):
        name = f.parent.name
        if name not in ids:
            continue
        lines = (yaml.safe_load(f.read_text()) or {}).get("zonelines") or {}
        for token, line in lines.items():
            to = (line or {}).get("to")
            if to in ids and len(str(token)) == 4:
                rows.append((ids[name], str(token), ids[to]))
    rows.sort()
    with open(out, "w") as o:
        o.write("# vana-client zonelines_gen.py from LandSandBoat data/zones/*/zone.yaml and data/enums/zone.yaml\n")
        o.write("# from-zone token to-zone\n")
        for r in rows:
            o.write("%d %s %d\n" % r)
    print("%d zone lines" % len(rows))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
