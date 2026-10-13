#!/bin/bash
# Creates the self-signed code signing certificate that release builds are signed
# with, so macOS keeps Screen Recording and Input Monitoring grants across updates.
#
#   scripts/make-signing-cert.sh [--import] [<out.p12>]
#
# Writes a password-protected .p12 (default ~/.tauri/recast-signing.p12) and prints
# the commands that store it as GitHub Actions secrets. The password comes from
# SIGNING_CERT_PASSWORD, or is asked for. --import also adds the certificate to the
# login keychain for local release builds.

set -euo pipefail

NAME="${SIGNING_CERT_NAME:-Recast Code Signing}"
DAYS=7300
REPO="${REPO:-abderra7mane/recast}"
# LibreSSL writes a .p12 that `security import` can read; OpenSSL 3 needs -legacy.
OPENSSL=/usr/bin/openssl

import=false
out=""
for arg in "$@"; do
  case "$arg" in
    --import) import=true ;;
    -*) echo "usage: $0 [--import] [<out.p12>]" >&2; exit 2 ;;
    *) out="$arg" ;;
  esac
done
out="${out:-$HOME/.tauri/recast-signing.p12}"

if [ -e "$out" ]; then
  echo "$out already exists, keeping it. Releases must keep using the same certificate." >&2
  exit 1
fi

password="${SIGNING_CERT_PASSWORD:-}"
if [ -z "$password" ]; then
  read -rsp "Password for the .p12: " password; echo
  read -rsp "Repeat it: " again; echo
  [ "$password" = "$again" ] || { echo "The passwords don't match." >&2; exit 1; }
fi
[ -n "$password" ] || { echo "The password must not be empty." >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "${work:?}"' EXIT

cat > "$work/cert.conf" <<EOF
[req]
distinguished_name = dn
prompt = no
x509_extensions = ext

[dn]
CN = $NAME

[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF

"$OPENSSL" req -x509 -newkey rsa:3072 -sha256 -nodes -days "$DAYS" \
  -config "$work/cert.conf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null

mkdir -p "$(dirname "$out")"
(
  umask 077
  P12_PASSWORD="$password" "$OPENSSL" pkcs12 -export -name "$NAME" \
    -inkey "$work/key.pem" -in "$work/cert.pem" -out "$out" -passout env:P12_PASSWORD
)

if $import; then
  security import "$out" -k "$HOME/Library/Keychains/login.keychain-db" \
    -P "$password" -T /usr/bin/codesign
fi

"$OPENSSL" x509 -in "$work/cert.pem" -noout -subject -enddate -fingerprint -sha1

cat <<EOF

Wrote $out. Back it up with its password: releases signed with another certificate
make every user grant Recast's permissions again.

Store it in GitHub Actions secrets (gh asks for the password):

  base64 -i "$out" | gh secret set MACOS_CERTIFICATE --repo $REPO
  gh secret set MACOS_CERTIFICATE_PASSWORD --repo $REPO

Or paste this as the MACOS_CERTIFICATE secret:

$(base64 -i "$out")
EOF

if $import; then
  echo
  echo "Imported into the login keychain. Sign local release builds with it:"
  echo "  make release-bundle RELEASE_SIGNING_IDENTITY=\"$NAME\""
fi
