#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 TAG RELEASE_DIRECTORY" >&2
  exit 2
}

[[ $# -eq 2 ]] || usage
tag=$1
release_dir=$2
repo=k18labs/relay
identity="https://github.com/${repo}/.github/workflows/release.yml@refs/tags/${tag}"
issuer=https://token.actions.githubusercontent.com
assets=(relay-linux-x86_64 relay-linux-aarch64)
manifests=(relay-linux-x86_64.sha256 relay-linux-aarch64.sha256 relay-sbom.spdx.json)

for command in sha256sum cosign gh python3; do
  command -v "$command" >/dev/null || {
    echo "Required command not found: $command" >&2
    exit 2
  }
done
[[ -d "$release_dir" ]] || { echo "Not a directory: $release_dir" >&2; exit 2; }

cd "$release_dir"
for asset in "${assets[@]}"; do
  [[ -s "$asset" ]] || { echo "Missing or empty asset: $asset" >&2; exit 1; }
  [[ -s "${asset}.sha256" ]] || { echo "Missing checksum: ${asset}.sha256" >&2; exit 1; }
  [[ -s "${asset}.sigstore.json" ]] || { echo "Missing signature bundle: ${asset}.sigstore.json" >&2; exit 1; }
  sha256sum --check "${asset}.sha256"
done

sbom=relay-sbom.spdx.json
[[ -s "$sbom" ]] || { echo "Missing or empty SPDX SBOM: $sbom" >&2; exit 1; }
[[ -s "${sbom}.sigstore.json" ]] || { echo "Missing SBOM signature bundle" >&2; exit 1; }
python3 - "$sbom" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    document = json.load(stream)
required = {
    "spdxVersion": "SPDX-2.3",
    "dataLicense": "CC0-1.0",
}
for field, expected in required.items():
    if document.get(field) != expected:
        raise SystemExit(f"Invalid SPDX JSON: {field} must be {expected}")
if not isinstance(document.get("SPDXID"), str) or not document["SPDXID"]:
    raise SystemExit("Invalid SPDX JSON: missing document SPDXID")
if not isinstance(document.get("packages"), list):
    raise SystemExit("Invalid SPDX JSON: packages must be an array")
print("SPDX JSON structure parsed")
PY

for asset in "${assets[@]}"; do
  cosign verify-blob "$asset" --bundle "${asset}.sigstore.json" \
    --certificate-identity "$identity" --certificate-oidc-issuer "$issuer"
done
for manifest in "${manifests[@]}"; do
  [[ -s "${manifest}.sigstore.json" ]] || { echo "Missing signature bundle: ${manifest}.sigstore.json" >&2; exit 1; }
  cosign verify-blob "$manifest" --bundle "${manifest}.sigstore.json" \
    --certificate-identity "$identity" --certificate-oidc-issuer "$issuer"
done
for asset in "${assets[@]}"; do
  gh attestation verify "$asset" --repo "$repo"
done

echo "Release artifacts for $tag passed checksum, signature, provenance, and SPDX JSON checks."
