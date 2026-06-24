#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

dist_dir="${LAKECAT_BOOK_DIST_DIR:-docs/book/dist}"
mkdir -p "$dist_dir"
dist_dir="$(cd "$dist_dir" && pwd)"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

# Pandoc's media extraction and its Haskell runtime use TMPDIR. Keep those
# transient files with the other per-run conversion state instead of allowing
# a converter default to write beside the source or into an operator profile.
export TMPDIR="$tmpdir"

# Pandoc stages media under its working directory even when TMPDIR is set.
# Execute it from the owned workspace while passing only absolute source and
# output paths, so conversion never creates scratch material beside the book.
run_pandoc() {
  (
    cd "$tmpdir"
    pandoc "$@"
  )
}

# Keep Calibre's conversion state out of the user's profile and the tracked
# artifact directory. Callers may override this for deliberate local debugging.
export CALIBRE_CONFIG_DIRECTORY="${CALIBRE_CONFIG_DIRECTORY:-$tmpdir/calibre-config}"
mkdir -p "$CALIBRE_CONFIG_DIRECTORY"

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

sed "s/{{KINDLE_NAME}}/$kindle_name/g" "$repo_root/docs/book/cover.md" > "$tmpdir/cover.md"

run_pandoc "$tmpdir/cover.md" \
  -o "$tmpdir/cover.pdf" \
  --pdf-engine=typst

run_pandoc "$repo_root/docs/book/lakecat.md" \
  -o "$tmpdir/body.pdf" \
  --pdf-engine=typst \
  --toc \
  --number-sections

pdfunite "$tmpdir/cover.pdf" "$tmpdir/body.pdf" "$dist_dir/lakecat.pdf"
docs/book/check_pdf_layout.sh "$dist_dir/lakecat.pdf"

run_pandoc "$tmpdir/cover.md" "$repo_root/docs/book/lakecat.md" \
  -o "$dist_dir/lakecat.epub" \
  --toc \
  --number-sections \
  --metadata-file "$repo_root/docs/book/metadata.yaml" \
  --metadata date="$pubdate" \
  --css "$repo_root/docs/book/epub.css" \
  --epub-title-page=false

docs/book/fix_epub_layout.sh "$dist_dir/lakecat.epub" "$kindle_name"
find "$dist_dir" -maxdepth 1 -name "$kindle_short_title (*).epub" -exec rm -f {} +
ln -s "$(basename "$stable_epub")" "$kindle_epub"
docs/book/check_epub_metadata.sh "$dist_dir/lakecat.epub" "$kindle_name"

/Applications/calibre.app/Contents/MacOS/ebook-convert \
  "$dist_dir/lakecat.epub" \
  "$dist_dir/lakecat.mobi"

scripts/check-book-artifact-contract.sh "$dist_dir"
