#!/bin/bash
# Scrape gwern.net essays as markdown files.
# Usage: ./scripts/scrape-gwern.sh [output-dir] [max-pages]
#
# Fetches the index page (markdown source), extracts internal essay links,
# then downloads each essay's markdown source into the output directory.

set -euo pipefail

OUT_DIR="${1:-/tmp/gwern-vault}"
MAX_PAGES="${2:-200}"

mkdir -p "$OUT_DIR"

echo "Fetching gwern.net index..."

# Grab the index page as markdown, extract internal links like (/some-path)
# Filter to top-level essay paths (no /doc/, /static/, no anchors-only, no external)
curl -sS -H "Accept: text/markdown" "https://gwern.net/index" \
  | grep -oE '\(/[a-zA-Z][a-zA-Z0-9_/-]*\)' \
  | sed 's/^(//; s/)$//' \
  | sort -u \
  | grep -v '^/doc/' \
  | grep -v '^/static/' \
  | grep -v '^/index' \
  | head -n "$MAX_PAGES" \
  > /tmp/gwern-paths.txt

TOTAL=$(wc -l < /tmp/gwern-paths.txt | tr -d ' ')
echo "Found $TOTAL essay paths (capped at $MAX_PAGES). Downloading..."

COUNT=0
ERRORS=0

while IFS= read -r path; do
  COUNT=$((COUNT + 1))
  # Convert path to filename: /scaling-hypothesis -> scaling-hypothesis.md
  filename=$(echo "$path" | sed 's|^/||; s|/|--|g').md

  printf "\r[%d/%d] %s" "$COUNT" "$TOTAL" "$path"

  if curl -sS -f -H "Accept: text/markdown" "https://gwern.net${path}" -o "$OUT_DIR/$filename" 2>/dev/null; then
    # Skip files that are too small (likely error pages) or empty
    size=$(wc -c < "$OUT_DIR/$filename" | tr -d ' ')
    if [ "$size" -lt 100 ]; then
      rm "$OUT_DIR/$filename"
      ERRORS=$((ERRORS + 1))
    fi
  else
    ERRORS=$((ERRORS + 1))
  fi

  # Be polite
  sleep 0.3
done < /tmp/gwern-paths.txt

echo ""
echo "Done! Downloaded $((COUNT - ERRORS)) essays to $OUT_DIR ($ERRORS skipped)"
echo ""
echo "To use as an Obsidian vault, open $OUT_DIR as a vault in Obsidian."
