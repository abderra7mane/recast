#!/bin/bash
# Runs a command with APPLE_SIGNING_IDENTITY set to the release signing identity.
#
#   scripts/with-signing-identity.sh <command> [<args>...]
#
# The identity comes from, in order:
#   - MACOS_CERTIFICATE (a base64 .p12 from scripts/make-signing-cert.sh) and
#     MACOS_CERTIFICATE_PASSWORD: imported into a temporary keychain, which is
#     deleted afterwards;
#   - RELEASE_SIGNING_IDENTITY: a code signing identity already in a keychain.
# Without either, the command signs ad hoc, which makes macOS forget the app's
# permissions on every update, so this fails under CI.

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "usage: $0 <command> [<args>...]" >&2
  exit 2
fi

# Tauri imports APPLE_CERTIFICATE itself but only accepts Apple-issued certificates.
unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD

if [ -n "${MACOS_CERTIFICATE:-}" ]; then
  if [ -z "${MACOS_CERTIFICATE_PASSWORD:-}" ]; then
    echo "MACOS_CERTIFICATE is set but MACOS_CERTIFICATE_PASSWORD is not." >&2
    exit 1
  fi

  work="$(mktemp -d)"
  keychain="$work/signing.keychain-db"
  keychain_password="$(/usr/bin/openssl rand -hex 24)"
  search_list=()
  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line#\"}"
    search_list+=("${line%\"}")
  done < <(security list-keychains -d user)

  cleanup() {
    security list-keychains -d user -s ${search_list[@]+"${search_list[@]}"} || true
    security delete-keychain "$keychain" 2>/dev/null || true
    rm -rf "${work:?}"
  }
  trap cleanup EXIT
  trap 'exit 130' INT TERM

  printf '%s' "$MACOS_CERTIFICATE" | base64 -D > "$work/cert.p12"
  security create-keychain -p "$keychain_password" "$keychain"
  security set-keychain-settings "$keychain"
  security unlock-keychain -p "$keychain_password" "$keychain"
  security import "$work/cert.p12" -k "$keychain" -P "$MACOS_CERTIFICATE_PASSWORD" \
    -T /usr/bin/codesign >/dev/null
  rm -f "$work/cert.p12"
  unset MACOS_CERTIFICATE MACOS_CERTIFICATE_PASSWORD
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
    -k "$keychain_password" "$keychain" >/dev/null
  # codesign only looks up identities in the search list.
  security list-keychains -d user -s "$keychain" ${search_list[@]+"${search_list[@]}"}

  identity="$(security find-identity -p codesigning "$keychain" |
    awk '$1 == "1)" { print $2; exit }')"
  if [ -z "$identity" ]; then
    echo "MACOS_CERTIFICATE has no code signing identity." >&2
    exit 1
  fi
  echo "Signing with the certificate from MACOS_CERTIFICATE ($identity)."
elif [ -n "${RELEASE_SIGNING_IDENTITY:-}" ]; then
  identity="$RELEASE_SIGNING_IDENTITY"
  echo "Signing with \"$identity\"."
elif [ -n "${CI:-}" ]; then
  echo "::error::Set MACOS_CERTIFICATE and MACOS_CERTIFICATE_PASSWORD; an ad-hoc signed release makes users grant permissions again." >&2
  exit 1
else
  identity="-"
  echo "warning: no MACOS_CERTIFICATE or RELEASE_SIGNING_IDENTITY, signing ad hoc." >&2
  echo "warning: macOS asks for permissions again after installing this build." >&2
fi

APPLE_SIGNING_IDENTITY="$identity" "$@"
