"""Shared helpers for the "ultimate build" surveys (../ultimate.md).

Every survey runs on `hwvx-x extract` trees of the releases, one folder per release:

    hwvx-x extract orig/gc/files                 SURVEY/gc
    hwvx-x extract <HWVX-SDL import>/pc          SURVEY/pc      (the pristine CD data)
    hwvx-x extract orig/ps2/XPS/ENGLISH          SURVEY/ps2
    hwvx-x extract orig/ps2-beta/XPS/ENGLISH     SURVEY/beta

The releases lay their archives out differently (the PC import is flat, the PS2 prototype is
upper-case), so archives are keyed by their lower-case file stem, packs by their folder name.
"""

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "ps2-version" / "scripts"))
sys.path.insert(0, str(HERE.parents[1] / "port-format" / "scripts"))

RELEASES = ("gc", "pc", "ps2")


def archives(root):
    """{archive stem (lower case): folder} for an extracted tree."""
    out = {}
    for m in sorted(Path(root).rglob("manifest.json")):
        out[m.parent.name.lower()] = m.parent
    return out


def packs(archive_folder):
    """[(pack folder name, folder, manifest entry)] in TOC order."""
    man = json.loads((Path(archive_folder) / "manifest.json").read_text())
    return [(e["path"], Path(archive_folder) / e["path"], e) for e in man["entries"]]


def category(archive):
    a = archive.lower()
    if a == "hw_cars":
        return "cars"
    if a in ("hw_ipack", "mcp", "hw_brief"):
        return "interface"
    if a in ("hw_items", "hw_share"):
        return "items/share"
    if a.startswith("hwb_m") or a == "hw_music":
        return "audio"
    return "worlds"


def survey_root(argv, default="HWVX-SDL/build-survey"):
    args = [a for a in argv if not a.startswith("--")]
    return Path(args[0] if args else default)
