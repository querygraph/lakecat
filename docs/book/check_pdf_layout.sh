#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 path/to/book.pdf" >&2
  exit 2
fi

pdf="$1"

if [[ ! -f "$pdf" ]]; then
  echo "PDF layout check failed: PDF not found: $pdf" >&2
  exit 2
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

page1="$tmpdir/page1.txt"
page2="$tmpdir/page2.txt"
page1_images="$tmpdir/page1-images.txt"

pdftotext -f 1 -l 1 "$pdf" "$page1"
pdftotext -f 2 -l 2 "$pdf" "$page2"
pdfimages -f 1 -l 1 -list "$pdf" > "$page1_images"

require_pattern() {
  local pattern="$1"
  local file="$2"
  local message="$3"

  if ! grep -Eq "$pattern" "$file"; then
    echo "PDF layout check failed: $message" >&2
    exit 1
  fi
}

reject_pattern() {
  local pattern="$1"
  local file="$2"
  local message="$3"

  if grep -Eq "$pattern" "$file"; then
    echo "PDF layout check failed: $message" >&2
    exit 1
  fi
}

require_pattern '^[[:space:]]*1[[:space:]]+[0-9]+[[:space:]]+image[[:space:]]+' "$page1_images" \
  "cover page does not contain the raster cover image"
reject_pattern '^Contents$' "$page1" "cover page includes body contents"
reject_pattern '^[0-9]+$' "$page1" "cover page has a standalone page number"

require_pattern '^Contents$' "$page2" "page 2 is not the contents page"
require_pattern '^1$' "$page2" "page 2 does not show body numbering started at 1"

echo "PDF layout check passed: $pdf"
