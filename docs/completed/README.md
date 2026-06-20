# Completed Design Reviews

This directory contains historical OPUS review and design notes.

The active LakeCat design record is now `../../DESIGN.md`. The OPUS files remain
here for audit and provenance only; do not use them as live instructions unless
you first reconcile the relevant point back into the root design, status, goal,
architecture, or agent guidance documents.

Consolidation was audited on 2026-06-19 and re-audited on 2026-06-20. The
durable OPUS findings, design decisions, and working-plan entries are
represented in `../../DESIGN.md`, especially its consolidation note, source
ledger, review log, closure map, and priority plan. This directory is
intentionally an archive, not a backlog.

## Archived OPUS Files

- `OPUS1.md`
- `OPUS1-DESIGN.md`
- `OPUS2.md`
- `OPUS2-DESIGN.md`

These files are deliberately archived here as of 2026-06-19. Their active
design content has been merged into `../../DESIGN.md` and adjacent canonical
docs; this directory is now provenance, not a work queue.

Each OPUS file carries an archive banner pointing back to `../../DESIGN.md`.
Future design changes should update the active docs directly and leave these
historical reviews frozen except for small archive-maintenance edits.

The active tree should not contain root-level or live OPUS design files. The
expected audit result is that `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`
returns no files, while `rg --files docs/completed -g 'OPUS*.md'` returns only
the four archived files above.

For a full consolidation audit, run:

```text
git ls-files 'OPUS*.md'
git ls-files 'docs/completed/OPUS*.md'
rg --files -g 'OPUS*.md' -g '!docs/completed/**'
rg --files docs/completed -g 'OPUS*.md'
```

The first command should return nothing. The second and fourth commands should
return only the four archived OPUS files listed above. The third command should
return nothing.

Current audited shape:

| Check | Expected result |
| --- | --- |
| Root `OPUS*.md` files | none |
| Archived `docs/completed/OPUS*.md` files | exactly the four files listed above |
| Active OPUS-numbered design plans | none |
| Active OPUS guidance | consolidated into `../../DESIGN.md` |

## Consolidation Ledger

| Archived file | Durable material now lives in |
| --- | --- |
| `OPUS1.md` | `../../DESIGN.md` `Review Log And Working Plan`, `OPUS Closure Map`, `Finding Status`, `Priority Plan`; `../../ARCHITECTURE.md` repo placement rules |
| `OPUS1-DESIGN.md` | `../../DESIGN.md` `Review Log And Working Plan`, `Thesis`, `Critical Path: The Restriction`, `OPUS Decisions Kept Permanent`; `../../ARCHITECTURE.md` Sail/Grust/TypeSec boundaries |
| `OPUS2.md` | `../../DESIGN.md` `Review Log And Working Plan`, `Current State`, `Finding Status`, `OPUS Closure Map`; `../../STATUS.md` latest verified slices |
| `OPUS2-DESIGN.md` | `../../DESIGN.md` `Review Log And Working Plan`, `Critical Path: The Restriction`, `Priority Plan`, `Review Gate`; `../../GOAL.md` durable operating objective |

## Archive Rules

- Treat the files in this directory as completed review artifacts.
- Do not append new working-plan entries to OPUS files.
- Do not create new OPUS-numbered active design files. Merge durable findings
  into `../../DESIGN.md` or the specific canonical doc first.
- When an archived detail becomes active again, move the durable guidance into
  `../../DESIGN.md` or the adjacent canonical doc first, then implement from
  there.
- Keep archive-maintenance edits small: link repairs, provenance notes, or
  explicit archive banners are fine; new LakeCat architecture belongs outside
  this directory.
