#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
release_dir="$temporary/release"
mock_bin="$temporary/mock-bin"
mkdir -p "$release_dir" "$mock_bin"

cat >"$mock_bin/cosign" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ ${1:-} == verify-blob ]] || exit 2
file=$2
shift 2
bundle= identity= issuer=
while [[ $# -gt 0 ]]; do
  case $1 in
    --bundle) bundle=$2; shift 2 ;;
    --certificate-identity) identity=$2; shift 2 ;;
    --certificate-oidc-issuer) issuer=$2; shift 2 ;;
    *) exit 2 ;;
  esac
done
[[ $identity == https://github.com/k18labs/relay/.github/workflows/release.yml@refs/tags/v0.1.0-rc.1 ]]
[[ $issuer == https://token.actions.githubusercontent.com ]]
[[ $(cat "$bundle") == "$(sha256sum "$file" | cut -d' ' -f1)" ]]
MOCK
cat >"$mock_bin/gh" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == "attestation verify "*" --repo k18labs/relay" ]]
MOCK
chmod +x "$mock_bin/cosign" "$mock_bin/gh"

create_fixture() {
  rm -rf "$release_dir"
  mkdir -p "$release_dir"
  for asset in relay-linux-x86_64 relay-linux-aarch64; do
    printf 'fixture binary %s\n' "$asset" >"$release_dir/$asset"
    (
      cd "$release_dir"
      sha256sum "$asset" >"${asset}.sha256"
    )
    sha256sum "$release_dir/$asset" | cut -d' ' -f1 >"$release_dir/${asset}.sigstore.json"
  done
  printf '{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0","SPDXID":"SPDXRef-DOCUMENT","packages":[]}\n' >"$release_dir/relay-sbom.spdx.json"
  for file in relay-linux-x86_64.sha256 relay-linux-aarch64.sha256 relay-sbom.spdx.json; do
    sha256sum "$release_dir/$file" | cut -d' ' -f1 >"$release_dir/${file}.sigstore.json"
  done
}

create_fixture
PATH="$mock_bin:$PATH" "$repo_root/scripts/release/verify-artifacts.sh" v0.1.0-rc.1 "$release_dir" >/dev/null

printf 'tampered\n' >>"$release_dir/relay-linux-x86_64"
if PATH="$mock_bin:$PATH" "$repo_root/scripts/release/verify-artifacts.sh" v0.1.0-rc.1 "$release_dir" >/dev/null 2>&1; then
  echo "Verifier accepted an altered binary" >&2
  exit 1
fi

create_fixture
printf '\n' >>"$release_dir/relay-linux-x86_64.sha256"
if PATH="$mock_bin:$PATH" "$repo_root/scripts/release/verify-artifacts.sh" v0.1.0-rc.1 "$release_dir" >/dev/null 2>&1; then
  echo "Verifier accepted an altered checksum manifest signature" >&2
  exit 1
fi

echo "Release verifier success and tamper-rejection checks passed (Cosign/GitHub CLIs mocked)."
