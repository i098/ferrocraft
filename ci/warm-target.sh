#!/bin/sh
# Make cargo's mtime freshness check follow git content, so a persistent CARGO_TARGET_DIR
# is reused after a fresh checkout. A file whose blob matches the last fully built tree gets
# an old mtime (cargo keeps its output); a changed or new file gets mtime now (cargo
# rebuilds it). No marker means every file is new: a full rebuild.
#   warm-target.sh start   before cargo: touch files, delete the marker
#   warm-target.sh done    after a successful build: record this tree as built
set -eu
[ -n "${CARGO_TARGET_DIR-}" ] || exit 0
mark=$CARGO_TARGET_DIR/.built-tree
tmp=${RUNNER_TEMP:-/tmp}
git -c core.quotepath=off ls-files -s | sort > "$tmp/tree.now"
case ${1-} in
start)
  mkdir -p "$CARGO_TARGET_DIR"
  [ -f "$mark" ] || : > "$mark"
  sort "$mark" > "$tmp/tree.old"
  rm -f "$mark"
  comm -13 "$tmp/tree.old" "$tmp/tree.now" | cut -f2- | tr '\n' '\0' | xargs -0 -r touch
  comm -12 "$tmp/tree.old" "$tmp/tree.now" | cut -f2- | tr '\n' '\0' | xargs -0 -r touch -d '2001-01-01 00:00:00'
  echo "warm-target: $(comm -12 "$tmp/tree.old" "$tmp/tree.now" | wc -l) files unchanged, $(comm -13 "$tmp/tree.old" "$tmp/tree.now" | wc -l) changed or new"
  ;;
done) cp "$tmp/tree.now" "$mark" ;;
*) echo "usage: warm-target.sh start|done" >&2; exit 2 ;;
esac
