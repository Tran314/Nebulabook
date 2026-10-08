#!/usr/bin/env bash
set -euo pipefail
repo='Tran314/Nebulabook'
tag='v4.1.0'
: "${EXPECTED_SHA:?Expected source commit required}"
test "${GITHUB_REPOSITORY:-}" = "$repo"
test "${GITHUB_REF:-}" = 'refs/heads/main'
test "$(gh api "repos/$repo/commits/main" --jq .sha)" = "$EXPECTED_SHA" || { echo 'main changed; refusing stale release'; exit 1; }
python3 scripts/verify-release.py release-assets "$EXPECTED_SHA"
# Only an actual HTTP 404 means absent; auth/network/5xx failures stop.
lookup() {
  local response code
  response=$(mktemp)
  if gh api --include "$1" > "$response"; then
    sed '1,/^[[:space:]]*$/d' "$response" > "$2"
    rm "$response"
    return 0
  else
    code=$(head -n 1 "$response" | awk '{print $2}')
    rm "$response"
    if test "$code" = 404; then return 4; fi
    return 1
  fi
}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if lookup "repos/$repo/git/ref/tags/$tag" "$work/tag.json"; then
  test "$(gh api "repos/$repo/commits/$tag" --jq .sha)" = "$EXPECTED_SHA" || { echo 'Existing tag belongs to another commit; no overwrite'; exit 1; }
else
  code=$?
  test "$code" = 4 || { echo 'Cannot verify tag absence'; exit 1; }
  gh api --method POST "repos/$repo/git/refs" -f "ref=refs/tags/$tag" -f "sha=$EXPECTED_SHA" > "$work/tag.json"
fi
if lookup "repos/$repo/releases/tags/$tag" "$work/existing.json"; then
  test "$(jq -r .draft "$work/existing.json")" = false || { echo 'Existing draft requires review; no overwrite'; exit 1; }
  mkdir "$work/existing"
  gh release download "$tag" --repo "$repo" --dir "$work/existing"
  python3 scripts/verify-release.py "$work/existing" "$EXPECTED_SHA" --check-sums
  cmp release-assets/SHA256SUMS.txt "$work/existing/SHA256SUMS.txt"
  echo 'Existing release matches this verified build; unchanged.'
  exit 0
else
  code=$?
  test "$code" = 4 || { echo 'Cannot verify release absence'; exit 1; }
fi
gh release create "$tag" release-assets/* --repo "$repo" --verify-tag --target "$EXPECTED_SHA" \
  --title 'Nebulabook v4.1.0 · Linux / Windows 原生记事本' --notes-file RELEASE_NOTES.md --draft
test "$(gh api "repos/$repo/commits/$tag" --jq .sha)" = "$EXPECTED_SHA" || { echo 'Tag changed; refusing publication'; exit 1; }
gh release edit "$tag" --repo "$repo" --draft=false
mkdir "$work/published"
gh release download "$tag" --repo "$repo" --dir "$work/published"
python3 scripts/verify-release.py "$work/published" "$EXPECTED_SHA" --check-sums
cmp release-assets/SHA256SUMS.txt "$work/published/SHA256SUMS.txt"
gh release view "$tag" --repo "$repo" --json url,tagName,assets
