# LakeCat Book Publishing

Use this runbook when updating, rebuilding, validating, delivering, or
publishing the LakeCat book in this repository.

## Source Layout

- Manuscript: `docs/book/lakecat.md`
- Cover source: `docs/book/cover.md`
- EPUB metadata: `docs/book/metadata.yaml`
- Build script: `docs/book/build.sh`
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
- `docs/book/dist/VERSION.md`

The Kindle-facing EPUB path is generated from `title_stem` in
`docs/book/metadata.yaml` and `[workspace.package].version` in `Cargo.toml`:

```text
lakecat (0.1.0).epub
```

That versioned path must be a symlink to the stable EPUB:

```text
docs/book/dist/lakecat (0.1.0).epub -> lakecat.epub
```

Track the stable EPUB, PDF, MOBI, and `VERSION.md` when generated deliverables
are part of the requested change. The versioned EPUB is a generated symlink and
`.gitignore` ignores future versioned EPUB names matching
`docs/book/dist/* (*).epub`.

`VERSION.md` must contain:

```yaml
kindle_name: lakecat (0.1.0)
built_at: YYYY-MM-DD
epub_file: lakecat.epub
kindle_link: lakecat (0.1.0).epub
```

## Metadata Rules

The visible book title stays clean:

```text
LakeCat
```

The Kindle/catalog title is versioned:

```text
lakecat (0.1.0)
```

Keep those surfaces separate:

- Cover, NCX, navigation title, and visible table of contents: `LakeCat`
- OPF `dc:title` and title-sort metadata: `lakecat (0.1.0)`
- Upload/delivery filename: `lakecat (0.1.0).epub`
- Dist marker: `VERSION.md`

Do not hard-code the version in the manuscript or cover. The cover uses
`{{KINDLE_NAME}}`, and `docs/book/build.sh` renders a temporary cover with the
current generated Kindle name.

## Cover Rules

The cover is a separate Markdown file with two raw blocks:

- Typst raw block for PDF.
- HTML raw block for EPUB and MOBI.

The Typst cover block must include:

```typst
#set page(margin: 1in, numbering: none)
```

This prevents a printed page number on the standalone cover. After merging, the
PDF should have:

- Page 1: cover text only, no printed page number.
- Page 2: Contents/body PDF, printed page number `1`.

For the EPUB cover, keep the HTML simple. Do not use flexbox. Kindle renderers
are more reliable with centered text and margins.

Keep code blocks compact in EPUB and MOBI through `docs/book/epub.css`.

## Build

From the repository root:

```sh
docs/book/build.sh
```

The build script:

1. Reads the workspace version from `Cargo.toml`.
2. Reads `title_stem` from `docs/book/metadata.yaml`.
3. Computes `kindle_name`, for example `lakecat (0.1.0)`.
4. Writes `docs/book/dist/VERSION.md`.
5. Renders a temporary cover with `{{KINDLE_NAME}}` replaced.
6. Builds a standalone cover PDF.
7. Builds the body PDF with table of contents and numbered sections.
8. Merges cover PDF before body PDF into `docs/book/dist/lakecat.pdf`.
9. Builds `docs/book/dist/lakecat.epub` with `--css docs/book/epub.css` and
   `--epub-title-page=false`.
10. Runs `fix_epub_layout.sh` to repair Pandoc EPUB defaults.
11. Creates the versioned EPUB symlink.
12. Runs `check_epub_metadata.sh`.
13. Converts the EPUB to `docs/book/dist/lakecat.mobi`.

Calibre is expected at:

```sh
/Applications/calibre.app/Contents/MacOS/ebook-convert
```

Use that app-bundle path unless the application bundle changes.

## EPUB Layout Fix

`docs/book/fix_epub_layout.sh` rewrites the generated EPUB so that:

- The custom cover XHTML is first in the spine.
- The navigation document follows it and is marked `linear="no"`.
- Pandoc's generated wrapper heading around the cover is removed.
- The cover XHTML body is marked as frontmatter.
- OPF `dc:title` and title-sort metadata are set to the Kindle/catalog title.

Keep `--epub-title-page=false` in the Pandoc EPUB command. Without it, Pandoc can
generate an extra empty `EPUB/text/title_page.xhtml` before the custom cover.

## Required Validation

After every build, run:

```sh
expected_title=$(awk -F': ' '/^kindle_name:/ { print $2 }' docs/book/dist/VERSION.md)
docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"
```

The validator rejects:

- Missing OPF title, creator, language, date, or modified metadata.
- Missing title-sort metadata.
- Fallback `UNTITLED` or `Unknown` metadata.
- Navigation or NCX titles that do not say `LakeCat`.
- A spine that does not put the cover before the nav item.
- A generated empty `title_page.xhtml`.
- A generated wrapper heading before the cover.
- Flexbox in the EPUB cover.
- Missing compact code-block rules in the EPUB stylesheet.
- Missing stable EPUB.
- A stable EPUB that differs from the canonical EPUB.
- A missing or non-symlink versioned Kindle EPUB.
- A versioned symlink that does not point to `lakecat.epub`.
- A missing or incomplete `VERSION.md`.

Also verify the PDF cover numbering:

```sh
pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -
pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -
```

Expected result:

- Page 1 extracts cover text and no standalone page number.
- Page 2 contains Contents and the body numbering starts at `1`.

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
lakecat (0.1.0)
```

## Delivery

For local iCloud delivery, copy the versioned symlink path by name:

```sh
kindle_link=$(awk -F': ' '/^kindle_link:/ { print $2 }' docs/book/dist/VERSION.md)
cp "docs/book/dist/$kindle_link" "$HOME/icloud/books/"
```

This produces a regular EPUB file at:

```text
~/icloud/books/lakecat (0.1.0).epub
```

That is intentional: the destination should preserve the versioned filename,
not the symlink relationship.

Do not treat iCloud delivery as a broad directory-access task. Derive the
current filename from `docs/book/dist/VERSION.md`, then use exact-path `stat`,
`cmp`, or `cp` against `~/icloud/books/<kindle_link>`.

## Git Delivery

When a publishing change affects source, metadata, build scripts, or generated
deliverables, commit the source changes and rebuilt artifacts together.

Before committing:

```sh
git status --short
git diff --check
```
