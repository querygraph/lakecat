#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

dist_dir="${LAKECAT_BOOK_DIST_DIR:-docs/book/dist}"
mkdir -p "$dist_dir"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

version="$(
  awk '
    /^\[workspace\.package\]/ { in_workspace_package = 1; next }
    /^\[/ { in_workspace_package = 0 }
    in_workspace_package && /^version[[:space:]]*=/ {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml
)"

if [[ -z "$version" ]]; then
  echo "could not read workspace package version from Cargo.toml" >&2
  exit 1
fi

pubdate="$(date -u +%F)"
kindle_short_title="$(
  if [[ -f docs/book/metadata.yaml ]]; then
    awk -F: '
      $1 ~ /^[[:space:]]*title_stem[[:space:]]*$/ {
        value = $2
        sub(/^[[:space:]]*/, "", value)
        sub(/[[:space:]]*$/, "", value)
        gsub(/^["'\'']|["'\'']$/, "", value)
        print value
        exit
      }
    ' docs/book/metadata.yaml
  fi
)"

if [[ -z "$kindle_short_title" ]]; then
  kindle_short_title="lakecat"
fi

kindle_name="$kindle_short_title ($version)"
stable_epub="$dist_dir/$kindle_short_title.epub"
kindle_epub="$dist_dir/$kindle_name.epub"

{
  printf 'kindle_name: %s\n' "$kindle_name"
  printf 'built_at: %s\n' "$pubdate"
  printf 'epub_file: %s.epub\n' "$kindle_short_title"
  printf 'kindle_link: %s.epub\n' "$kindle_name"
} > "$dist_dir/VERSION.md"

sed "s/{{KINDLE_NAME}}/$kindle_name/g" docs/book/cover.md > "$tmpdir/cover.md"

pandoc "$tmpdir/cover.md" \
  -o "$tmpdir/cover.pdf" \
  --pdf-engine=typst

pandoc docs/book/lakecat.md \
  -o "$tmpdir/body.pdf" \
  --pdf-engine=typst \
  --toc \
  --number-sections

pdfunite "$tmpdir/cover.pdf" "$tmpdir/body.pdf" "$dist_dir/lakecat.pdf"
docs/book/check_pdf_layout.sh "$dist_dir/lakecat.pdf"

pandoc "$tmpdir/cover.md" docs/book/lakecat.md \
  -o "$dist_dir/lakecat.epub" \
  --toc \
  --number-sections \
  --metadata-file docs/book/metadata.yaml \
  --metadata date="$pubdate" \
  --css docs/book/epub.css \
  --epub-title-page=false

docs/book/fix_epub_layout.sh "$dist_dir/lakecat.epub" "$kindle_name"
find "$dist_dir" -maxdepth 1 -name "$kindle_short_title (*).epub" -exec rm -f {} +
ln -s "$(basename "$stable_epub")" "$kindle_epub"
docs/book/check_epub_metadata.sh "$dist_dir/lakecat.epub" "$kindle_name"

/Applications/calibre.app/Contents/MacOS/ebook-convert \
  "$dist_dir/lakecat.epub" \
  "$dist_dir/lakecat.mobi"

scripts/check-book-artifact-contract.sh "$dist_dir"
