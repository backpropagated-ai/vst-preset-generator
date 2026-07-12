"""Regenerate surge_init_data.rs from the installed Surge XT 1.3.4 factory Init Saw.fxp."""
import re
import struct
import sys
from pathlib import Path

FXP = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / "Library/Application Support/Surge XT/patches_factory/Templates/Init Saw.fxp"
OUT = Path(__file__).resolve().parent.parent / "preset-core/src/writers/surge_init_data.rs"

data = FXP.read_bytes()
assert data[0:4] == b"CcnK" and data[8:12] == b"FPCh" and data[12:16].hex() and data[16:20] == b"cjs3", \
    f"unexpected fxp header: {data[:20].hex()}"

# FXP header is 0x3C bytes, then sub3 patch header: tag(4) + xmlsize(4 LE) + wtsize[2][3](24)
off = 0x3C
tag = data[off:off + 4]
assert tag == b"sub3", f"expected sub3 tag, got {tag!r}"
xmlsize = struct.unpack_from("<I", data, off + 4)[0]
xml_start = off + 4 + 4 + 24
xml = data[xml_start:xml_start + xmlsize].decode("utf-8")

m = re.search(r'<patch revision="(\d+)"', xml)
revision = int(m.group(1))
print(f"source patch revision: {revision}, xmlsize: {xmlsize}")

params_span = re.search(r"<parameters>(.*?)</parameters>", xml, re.S).group(1)
entries = []
for pm in re.finditer(r'<(\w+)\s+type="(\d+)"\s+value="([^"]*)"\s*([^/>]*?)\s*/>', params_span):
    name, ty, val, tail = pm.group(1), pm.group(2), pm.group(3), pm.group(4).strip()
    entries.append((name, ty, val, tail))
print(f"extracted {len(entries)} parameters")

# Sanity: mapped override names must exist in the table
required = ["a_osc1_type", "a_osc1_pitch", "a_level_o1", "a_mute_o1", "a_filter1_cutoff", "a_filter1_resonance", "a_env1_attack", "a_env2_sustain", "a_lfo0_rate", "a_volume"]
names = {e[0] for e in entries}
missing = [r for r in required if r not in names]
assert not missing, f"missing required override targets: {missing}"

lines = [
    "// SPDX-License-Identifier: GPL-3.0-only",
    "// Copyright (c) 2026 DeepSynth AI",
    "//",
    "// Auto-extracted from the Surge XT 1.3.4 factory `Templates/Init Saw.fxp`",
    f"// (GPL-3.0, Surge Synth Team) — patch streaming revision {revision}, matching the",
    "// current public Surge XT release so generated patches load without a version",
    "// warning. Full parameter set with exact storage names, value types, default",
    "// raw values, and trailing attributes as Surge XT writes them. Every generated",
    "// patch starts from these defaults and overrides only the mapped subset.",
    "//",
    "// DO NOT EDIT BY HAND — regenerate with scripts/regen_surge_init.py against a",
    "// Surge XT factory Init Saw patch if the format changes.",
    "",
    "/// One Surge patch parameter: `(storage_name, value_type, default_value, trailing_attrs)`.",
    "/// `value_type` is Surge's `vt_int`=0 / `vt_float`=2. `trailing_attrs` is the raw XML",
    "/// attribute string Surge appends after `value` (already quote-escaped), or empty.",
    "pub(crate) type SurgeInitParam = (&'static str, u8, &'static str, &'static str);",
    "",
    f"/// Patch streaming revision of the source Init Saw patch ({revision} = Surge XT 1.3.4).",
    f"pub(crate) const SURGE_INIT_REVISION: u32 = {revision};",
    "",
    "pub(crate) const SURGE_INIT_PARAMS: &[SurgeInitParam] = &[",
]
for name, ty, val, tail in entries:
    tail_esc = tail.replace("\\", "\\\\").replace('"', '\\"')
    val_esc = val.replace("\\", "\\\\").replace('"', '\\"')
    lines.append(f'    ("{name}", {ty}, "{val_esc}", "{tail_esc}"),')
lines.append("];")
lines.append("")

OUT.write_text("\n".join(lines))
print(f"wrote {OUT} ({len(entries)} params, revision {revision})")
