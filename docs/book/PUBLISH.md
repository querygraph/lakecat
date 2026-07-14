# LakeCat Book Publishing

Use this runbook when updating, rebuilding, validating, delivering, or
publishing the LakeCat book in this repository.

## Source Layout

- Manuscript: `docs/book/lakecat.md`
- Final cover: `cover/lakecat-cover.png`
- Cover recipe and source assets: `cover/README.md`
- Browser-reader cover wrapper: `docs/book/cover.md`
- EPUB metadata: `docs/book/metadata.yaml`
- Build script: `docs/book/build.sh`
- Shared configuration: `book.build.json`
- EPUB layout fixer: `docs/book/fix_epub_layout.sh`
- EPUB validator: `docs/book/check_epub_metadata.sh`
- Final artifacts: `docs/book/dist/`

The book directory is `docs/book/` in this repository. There is no top-level
`book/` directory in the current tree.

## Current Artifact Contract

The stable deliverables are:

- `docs/book/dist/lakecat.pdf`
- `docs/book/dist/lakecat.epub`
- `docs/book/dist/lakecat.mobi`
- `docs/book/dist/lakecat.html`
- `docs/book/dist/VERSION.md`

The versioned artifact stem is generated from `title_stem` in
`docs/book/metadata.yaml`, `[workspace.package].version` in `Cargo.toml`,
and the short Git commit:

```text
lakecat (0.3.0-abcdef12)
```

The EPUB, PDF, and HTML versioned paths must be symlinks to the stable files:

```text
docs/book/dist/lakecat (0.3.0-abcdef12).epub -> lakecat.epub
docs/book/dist/lakecat (0.3.0-abcdef12).pdf  -> lakecat.pdf
docs/book/dist/lakecat (0.3.0-abcdef12).html -> lakecat.html
```

Track the stable EPUB, PDF, MOBI, HTML, and `VERSION.md` when generated
deliverables are part of the requested change. Versioned EPUB/PDF/HTML names are
generated symlinks and are ignored.

`VERSION.md` must contain:

```yaml
kindle_name: lakecat (0.3.0-abcdef12)
built_at: YYYY-MM-DD
epub_file: lakecat.epub
kindle_link: lakecat (0.3.0-abcdef12).epub
html_file: lakecat.html
html_link: lakecat (0.3.0-abcdef12).html
html_title: LakeCat
```

## Metadata Rules

The visible book title stays clean:

```text
LakeCat
```

The Kindle/catalog title is versioned:

```text
lakecat (0.3.0-abcdef12)
```

The Ocelot edition subtitle is:

```text
Ocelot: Governed Iceberg REST with Proof Built In
```

Keep those surfaces separate:

- Cover, NCX, navigation title, and visible table of contents: `LakeCat`
- Cover and metadata subtitle: `Ocelot: Governed Iceberg REST with Proof Built In`
- Cover and package author: `Alexy Khrabrov` only
- OPF `dc:title` and title-sort metadata: `lakecat (0.3.0-abcdef12)`
- Upload/delivery filename: `lakecat (0.3.0-abcdef12).epub`
- Browser document title: `LakeCat`
- Dist marker: `VERSION.md`

Do not put the package version on the visible cover. The deterministic composer
owns its exact title, subtitle, author, and First Pair Press seal; the shared
builder owns the versioned Kindle/catalog metadata.

## Cover Rules

The canonical cover is the 1024x1536 raster image
`cover/lakecat-cover.png`. It is composed from the source LakeCat headboard,
generated portrait artwork, and reusable First Pair Press publisher mask
documented in `cover/README.md`. Rebuild it deterministically with:

```sh
uv run --no-project --with pillow python cover/make-cover.py
```

`book.build.json` installs the image as page 1 of the PDF and as the EPUB
`cover-image`. `docs/book/cover.md` is the browser-HTML title-page wrapper and
must reference the same image. Keep lettering out of generated portrait art;
`cover/make-cover.py` owns the exact white-on-dark title, subtitle,
`ALEXY KHRABROV` author line, and First Pair Press seal.

After merging, the PDF should have an image-only, unnumbered cover on page 1
and the numbered Contents page on page 2. The EPUB validator requires the
packaged cover bytes to match `cover/lakecat-cover.png` exactly.

Keep code blocks compact in EPUB, MOBI, and HTML through
`docs/book/epub.css`.

## Build

From the repository root:

```sh
docs/book/build.sh
```

The repository wrapper delegates to
`~/src/firstpair/publishing/scripts/build-library-book.sh`. The checked-in
`book.build.json` retains LakeCat's tracked diagram renderer, EPUB repair, and
local artifact validators. FirstPair owns rendering, complete manifest and
link generation, and mandatory PDF/EPUB/HTML verification. Building no longer
copies artifacts to iCloud; delivery remains a separate publishing action.

The shared build:

1. Reads the workspace version from `Cargo.toml`.
2. Reads `title_stem` from `docs/book/metadata.yaml`.
3. Computes `kindle_name`, for example `lakecat (0.3.0-abcdef12)`.
4. Writes `docs/book/dist/VERSION.md`.
5. Builds a standalone PDF page from `cover/lakecat-cover.png`.
6. Builds the body PDF with table of contents and numbered sections.
7. Merges the raster cover page before the body in `docs/book/dist/lakecat.pdf`.
8. Builds `docs/book/dist/lakecat.epub` with the same PNG as its cover image,
   `--css docs/book/epub.css`, and
   `--epub-title-page=false`.
9. Runs `fix_epub_layout.sh` to repair Pandoc EPUB defaults.
10. Creates the versioned artifact symlinks and full `VERSION.md` manifest.
11. Runs `check_epub_metadata.sh`.
12. Builds single-file and chapter HTML, packaging the cover with the chapters.
13. Converts the EPUB to `docs/book/dist/lakecat.mobi`.
14. Runs the complete book artifact contract, including HTML and PDF layout.

Calibre is expected at:

```sh
/Applications/calibre.app/Contents/MacOS/ebook-convert
```

Use that app-bundle path unless the application bundle changes.

## EPUB Layout Fix

`docs/book/fix_epub_layout.sh` rewrites the generated EPUB so that:

- Pandoc's image-cover XHTML is first in the spine.
- The navigation document follows it and is marked `linear="no"`.
- The first manuscript chapter follows the navigation document.
- OPF `dc:title` and title-sort metadata are set to the Kindle/catalog title.

Keep `--epub-title-page=false` in the Pandoc EPUB command. Without it, Pandoc can
generate an extra empty `EPUB/text/title_page.xhtml` before the image cover.

## Required Validation

After every build, run:

```sh
scripts/check-release-version-contract.sh
scripts/check-book-artifact-contract.sh docs/book/dist
expected_title=$(awk -F': ' '/^kindle_name:/ { print $2 }' docs/book/dist/VERSION.md)
docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"
docs/book/check_pdf_layout.sh docs/book/dist/lakecat.pdf
```

The release version contract rejects drift between `[workspace.package].version`
in `Cargo.toml`, all `crates/lakecat-*` package versions resolved by Cargo
metadata, the `RELEASE.md` tag command, `docs/book/dist/VERSION.md`, and the
versioned Kindle EPUB symlink.
The book artifact contract accepts an optional dist directory and validates the
same stable EPUB/PDF/MOBI/HTML files, marker fields, versioned EPUB/PDF/HTML
symlinks, EPUB metadata, HTML title/TOC, and PDF layout that release-candidate
mode generates out of tree.

The validator rejects:

- Missing OPF title, creator, language, date, or modified metadata.
- Missing title-sort metadata.
- Fallback `UNTITLED` or `Unknown` metadata.
- Navigation or NCX titles that do not say `LakeCat`.
- A spine that does not put the image cover before the nav item.
- A generated empty `title_page.xhtml`.
- Missing cover metadata, the wrong 1024x1536 SVG wrapper, or packaged cover
  bytes that differ from `cover/lakecat-cover.png`.
- Creator metadata other than `Alexy Khrabrov`, or publisher metadata other
  than `First Pair Press`.
- Missing compact code-block rules in the EPUB stylesheet.
- Missing stable EPUB.
- A stable EPUB that differs from the canonical EPUB.
- A missing or non-symlink versioned Kindle EPUB.
- A versioned symlink that does not point to `lakecat.epub`.
- Missing stable HTML, its `LakeCat` document title, or its generated TOC.
- Missing or incorrect versioned PDF/HTML symlinks.
- A missing or incomplete `VERSION.md`.

The PDF validator rejects a page 1 without a raster cover image, a cover page
that includes body contents or a standalone page number, and a page 2 that is
not Contents with body numbering started at `1`.

Check the versioned EPUB link:

```sh
kindle_link=$(awk -F': ' '/^kindle_link:/ { print $2 }' docs/book/dist/VERSION.md)
readlink "docs/book/dist/$kindle_link"
```

Expected result:

```text
lakecat.epub
```

Optional Calibre metadata check:

```sh
/Applications/calibre.app/Contents/MacOS/ebook-meta docs/book/dist/lakecat.epub
```

Expected title and title sort:

```text
lakecat (0.3.0-abcdef12)
```

## Delivery

For local iCloud delivery, copy the versioned symlink path by name:

```sh
kindle_link=$(awk -F': ' '/^kindle_link:/ { print $2 }' docs/book/dist/VERSION.md)
cp "docs/book/dist/$kindle_link" "$HOME/icloud/books/"
```

This produces a regular EPUB file at:

```text
~/icloud/books/lakecat (0.3.0-abcdef12).epub
```

That is intentional: the destination should preserve the versioned filename,
not the symlink relationship.

Do not treat iCloud delivery as a broad directory-access task. Derive the
current filename from `docs/book/dist/VERSION.md`, then use exact-path `stat`,
`cmp`, or `cp` against `~/icloud/books/<kindle_link>`.

## Blog Posts and TextPacks

Every blog post under `docs/blog/` ships with a Ulysses **`.textpack`** as its
hand-off deliverable to the writing/publishing app. **Always create a textpack
for each blog post**, following the procedure in `TEXTPACK.md` (repository root).

The short version (see `TEXTPACK.md` for the full steps and scripts):

1. **Reflow** the post's prose to one line per paragraph — hard wrapping renders
   ragged with vertical gaps in Ulysses/Ghost; leave code, lists, headings,
   blockquotes, tables, and image lines untouched.
2. **Render any `mermaid` diagrams to PNG** — Ulysses and Ghost do not render
   `mermaid`. LakeCat already renders mermaid for the book via
   `docs/book/render-diagrams.mjs` (white background, 2× scale, through
   `docs/book/puppeteer-config.json`); render blog diagrams the same way and
   reference the PNGs.
3. **Bundle** the Markdown plus image assets into a `<post>.textbundle` and zip it
   to `<post>.textpack` (with the `.textbundle` directory as the top-level entry).
4. **Do not commit** the `.textpack` (or any base64 fallback) — it duplicates the
   PNGs and bloats git. Generate it as a deliverable (e.g. under `/tmp`) and keep
   the repo source clean: the post `.md` plus any `diagrams/*.mmd` and `*.png`.

## Git Delivery

When a publishing change affects source, metadata, build scripts, or generated
deliverables, commit the source changes and rebuilt artifacts together.

Before committing:

```sh
git status --short
git diff --check
```
