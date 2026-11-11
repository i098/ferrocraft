#!/bin/sh
# Install ferrocraft: download the release archive for this OS and run its installer.
set -eu
repo=i098/ferrocraft
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  *) echo "ferrocraft: no release for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac
asset=ferrocraft-$target.tar.gz
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$asset" "https://github.com/$repo/releases/latest/download/$asset" ||
  { echo "ferrocraft: download failed." >&2; exit 1; }
tar -xzf "$tmp/$asset" -C "$tmp"
"$tmp/ferrocraft-launcher" install
