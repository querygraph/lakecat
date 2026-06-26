# LakeCat Releases

LakeCat uses ordinary SemVer tags for compatibility and feline codenames for
human release identity. A codename never replaces the SemVer tag or changes an
existing release commit.

## Released

| SemVer | Codename | Scope |
| --- | --- | --- |
| `v0.1.0` | Bobcat | Original Rust catalog foundation: Iceberg REST surface, Turso catalog spine, governed Sail planning boundary, TypeSec receipts, Grust projection boundary, and QGLake acceptance. |
| `v0.2.0` | Lynx | Published Sail helper bridge (Iceberg planning/model exposure + commit-table seam, manifest-bounds round-trip fix), Turso MVCC concurrent writes, the human-reviewability refactor (no monolithic source files), and a reconciled book and release-gate surface. |
| `v0.2.1` | Lynx | Maintenance: Turso/object-store commit-path performance (cached per-bucket object-store clients, pooled pragma-warmed write connections), dependency modernization (published TypeSec/Grust 0.11), the `catalog-commit-bench` harness and book chapter, and the `qglake-bundle` DRY extraction of the QueryGraph bootstrap-bundle wire contract. No wire-format or governance change vs `v0.2.0`. |

`bobcat` is an annotated companion tag for the immutable `v0.1.0` commit. It
exists for discovery only; package versioning and dependency resolution use the
SemVer tag and crate version.

## Planned Codenames

The names below are planning labels, not promises of scope or date. A release
is cut only after its SemVer, notes, local proof, and compatibility evidence are
ready.

| Planned line | Codename | Intended emphasis |
| --- | --- | --- |
| `0.3` | Ocelot | Governed access and TypeSec capability hardening. |
| `0.4` | Caracal | QueryGraph/QGLake operational integration. |
| `0.5` | Serval | Catalog management and durable tenancy operations. |
| `0.6` | Puma | Scale, observability, and deployment hardening. |
| `0.7` | Leopard | Cross-catalog interoperability and migration tooling. |
| `0.8` | Jaguar | Mature agentic workflow and evidence composition. |
| `0.9` | Tiger | Release-candidate stabilization toward `1.0`. |
| `1.0` | Lion | Stable LakeCat catalog foundation. |

Typed Iceberg v4 is intentionally not assigned a feline release. It enters a
future Sail-led release only after Apache Iceberg formally adopts the format and
Sail provides stable typed semantics.
