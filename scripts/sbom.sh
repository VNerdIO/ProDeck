#!/usr/bin/env bash
# Regenerate sbom.txt (local only; git- and docker-ignored) from the resolved Cargo graph.
# Run after any dependency change: scripts/sbom.sh
set -euo pipefail
cd "$(dirname "$0")/.."
cargo metadata --format-version 1 --locked 2>/dev/null > /tmp/prodeck-metadata.$$ || cargo metadata --format-version 1 > /tmp/prodeck-metadata.$$
python3 - /tmp/prodeck-metadata.$$ > sbom.txt <<'PY'
import json, sys, datetime
m = json.load(open(sys.argv[1]))
ws = set(m["workspace_members"])
pkgs = sorted(m["packages"], key=lambda p: (p["name"], p["version"]))
print("ProDeck software bill of materials")
print(f"Generated: {datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d %H:%M UTC')}")
print("Source: Cargo.lock (all targets, including wasm client and dev-dependencies)")
print(f"Packages: {len(pkgs)} ({sum(1 for p in pkgs if p['id'] in ws)} first-party)")
print()
print(f"{'NAME':<40} {'VERSION':<18} {'LICENSE':<40} SOURCE")
for p in pkgs:
    src = "workspace" if p["id"] in ws else ("crates.io" if (p.get("source") or "").startswith("registry+") else (p.get("source") or "local"))
    lic = p.get("license") or ("see " + p["license_file"] if p.get("license_file") else "UNKNOWN")
    print(f"{p['name']:<40} {p['version']:<18} {lic:<40} {src}")
PY
rm -f /tmp/prodeck-metadata.$$
echo "wrote sbom.txt ($(grep -m1 "^Packages:" sbom.txt | cut -d" " -f2) packages)"
