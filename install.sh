#!/bin/sh
# NextPlane installer: downloads a release, checks its SHA-256, puts `nextplane` in
# ~/.local/bin and installs the user service, which prints a one-time setup link.
#
#   curl -fsSL https://raw.githubusercontent.com/OWNER/nextplane/main/install.sh | sh
#   ... | sh -s -- --version=v0.1.0 --no-service
set -eu

REPO="${NEXTPLANE_REPO:-azamrizki/nextplane}"
VERSION=""
SERVICE=1
for arg in "$@"; do
  case "$arg" in
    --version=*) VERSION="${arg#--version=}" ;;
    --no-service) SERVICE=0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=darwin ;;
  *) echo "NextPlane builds exist for Linux and macOS only" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) ARCH=amd64 ;;
  aarch64 | arm64) ARCH=arm64 ;;
  *) echo "no build for $(uname -m)" >&2; exit 1 ;;
esac
ASSET="nextplane-$OS-$ARCH.tar.gz"

if [ -n "$VERSION" ]; then
  BASE="https://github.com/$REPO/releases/download/$VERSION"
else
  BASE="https://github.com/$REPO/releases/latest/download"
fi

TMP="$(mktemp -d)"
trap 'rm -f "$TMP/$ASSET" "$TMP/SHA256SUMS" "$TMP/nextplane"; rmdir "$TMP" 2>/dev/null || true' EXIT

echo "Downloading $ASSET ..."
curl -fsSL -o "$TMP/$ASSET" "$BASE/$ASSET"
curl -fsSL -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS"

WANT="$(awk -v n="$ASSET" '$2 == n || $2 == "*"n { print $1 }' "$TMP/SHA256SUMS")"
if command -v sha256sum >/dev/null 2>&1; then
  GOT="$(sha256sum "$TMP/$ASSET" | awk '{ print $1 }')"
else
  GOT="$(shasum -a 256 "$TMP/$ASSET" | awk '{ print $1 }')"
fi
if [ -z "$WANT" ] || [ "$WANT" != "$GOT" ]; then
  echo "checksum mismatch for $ASSET (expected '$WANT', got '$GOT'); nothing installed" >&2
  exit 1
fi

tar -xzf "$TMP/$ASSET" -C "$TMP" --strip-components=1 "nextplane-$OS-$ARCH/nextplane"
mkdir -p "$HOME/.local/bin"
mv "$TMP/nextplane" "$HOME/.local/bin/nextplane.new"
chmod 755 "$HOME/.local/bin/nextplane.new"
mv "$HOME/.local/bin/nextplane.new" "$HOME/.local/bin/nextplane"
echo "Installed $HOME/.local/bin/nextplane"

if [ "$SERVICE" = 1 ]; then
  "$HOME/.local/bin/nextplane" install
  echo
  echo "Open the setup link from the log to finish:  nextplane logs"
  echo "(or run 'nextplane setup-link' for a new one)"
else
  echo "Run 'nextplane serve' to start, or 'nextplane install' for a service."
fi
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "Add ~/.local/bin to your PATH to run 'nextplane' directly." ;;
esac
