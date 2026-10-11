#!/usr/bin/env bash
# Publishes a built site into the gh-pages branch, which GitHub Pages serves.
#
#   publish-pages.sh DEST [SITE_DIR]
#
# DEST is "." for the site root (main's build; the pr/ previews are kept) or a
# folder such as pr/35. With SITE_DIR, DEST is replaced by its contents;
# without it, DEST is removed. gh-pages is kept to a single commit, so old
# builds do not pile up in history. Concurrent publishes retry on a lost race.
set -euo pipefail

dest=${1:?usage: publish-pages.sh DEST [SITE_DIR]}
site=${2:-}
branch=gh-pages

for attempt in 1 2 3 4 5; do
  work=$(mktemp -d)
  index=$(mktemp -u)
  base=
  ref=refs/publish/$$
  if git fetch --quiet --depth=1 origin "+$branch:$ref" 2>/dev/null; then
    base=$(git rev-parse "$ref")
    git update-ref -d "$ref"
    git archive "$base" | tar -x -C "$work"
  fi
  if [ "$dest" = "." ]; then
    find "$work" -mindepth 1 -maxdepth 1 ! -name pr -exec rm -rf {} +
  else
    rm -rf "${work:?}/$dest"
  fi
  if [ -n "$site" ]; then
    mkdir -p "$work/$dest"
    cp -r "$site"/. "$work/$dest"/
  fi
  # Serve files as they are, without Jekyll.
  touch "$work/.nojekyll"
  GIT_INDEX_FILE=$index git --work-tree="$work" add -A .
  tree=$(GIT_INDEX_FILE=$index git write-tree)
  commit=$(git commit-tree "$tree" -m "Publish ${dest} from ${GITHUB_SHA:-local}")
  rm -rf "$work" "$index"
  if git push --quiet --force-with-lease="refs/heads/$branch:$base" origin "$commit:refs/heads/$branch"; then
    echo "Published ${dest} as ${commit::7}"
    exit 0
  fi
  echo "gh-pages moved; retrying ($attempt)"
  sleep $((attempt * 3 + RANDOM % 5))
done
echo "Could not publish ${dest}" >&2
exit 1
