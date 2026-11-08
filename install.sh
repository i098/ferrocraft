#!/bin/sh
# Install ferrocraft: download the release archive for this OS and run its installer.
# Needs `gh` (logged in) while the repo is private; falls back to curl once it is public.
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
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  gh release download --repo "$repo" --pattern "$asset" --dir "$tmp"
else
  curl -fsSL -o "$tmp/$asset" "https://github.com/$repo/releases/latest/download/$asset" ||
    { echo "ferrocraft: download failed. While the repo is private, run 'gh auth login' first." >&2; exit 1; }
fi
tar -xzf "$tmp/$asset" -C "$tmp"
"$tmp/ferrocraft-launcher" install
