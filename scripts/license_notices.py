#!/usr/bin/env python3
"""Generate bundled license notices from the locked, locally downloaded Cargo sources."""
import json
from pathlib import Path
import subprocess

metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"]))
parts = ["# Third-party notices\n\nGenerated from Cargo.lock. Some target-specific packages may not be linked in every binary.\n\nThe embedded Noto Sans font uses SIL OFL; see assets/OFL.txt / FONT-LICENSE.txt.\n"]
for package in sorted(metadata["packages"], key=lambda p:(p["name"],p["version"])):
    if package["name"] == "tpreview":
        continue
    parts.append(f'\n## {package["name"]} {package["version"]}\n\nLicense: {package.get("license") or "see package license"}\n\nSource: {package.get("repository") or "https://crates.io/crates/"+package["name"]}\n')
    root = Path(package["manifest_path"]).parent
    files = sorted(p for p in root.iterdir() if p.is_file() and p.name.lower().startswith(("license", "licence", "copying", "notice", "copyright")))
    if package.get("license_file"):
        files.append(root / package["license_file"])
    for file in dict.fromkeys(files):
        parts.append(f'\n### {file.name}\n\n```text\n{chr(10).join(line.rstrip() for line in file.read_text(errors="replace").strip().splitlines())}\n```\n')
Path("THIRD_PARTY_NOTICES.md").write_text("".join(parts))
