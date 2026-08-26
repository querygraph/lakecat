# Phase 0 Baseline and Upstream Audit

Audit date: **2026-08-26** (`America/New_York`). This document distinguishes
verified historical evidence, current source state, selected future pins, and work
that could not be rerun. It does not promote a smoke check into a benchmark result.

## Repository inventory

| Repository | Audited revision | Branch | Program role |
| --- | --- | --- | --- |
| LakeCat | `54ad9dcb7c941781a77c5210394924a2ae88a459` (`v0.3.0-10-g54ad9dcb`) | `master` | Catalog implementation and control-plane boundary. |
| catalog-bench | `d5c3c7f11f79267efe2044b80cbfa9ef9ad73452` | `master` | Existing neutral commit, write, cache, engine, and stock-client harness; canonical future lab owner. |
| QueryGraph | `32c28c46045a81f75da9118ffd307dcb589158db` | `main` | End-to-end semantic workflow and verified answers. |
| Sail | `dbff52b0dfff5fed302d09a72eeb7feb92f50725` | `lakecat` | Reusable Iceberg and execution behavior. Its unrelated untracked `output/` was not touched. |
| Grust | `5e7cdd5cdbb39e56ce8685f1993de24c329f4f65` | `agent/marciana-production-path` | Semantic graph behavior. |
| TypeSec | `669e105601c46aab0d11bcaaa4b06369f43bc934` | `agent/performance-benchmarks` | Governance semantics and receipts. |

There is no separate `catalog-commit-bench` checkout. The commit driver is the
`crates/commit` member of `querygraph/catalog-bench`; references to the former
repository name are stale documentation.

## Published 2026-08-08 evidence

The latest tracked commit-path result is a six-round Linux ARM64 run. Round one is
conditioning; rounds two through six determine medians. Every catalog used the
same Docker network and MinIO bucket. A valid row required a zero exit status, zero
non-conflict request errors, and object growth of at least `50 + 1000 +
concurrent_ok`.

| Component | Exact historical identity |
| --- | --- |
| Driver source | `catalog-bench@fbdf684566edb877abca94629ff702c93d6ca2fb` |
| LakeCat source | `lakecat@3cca8d1c749fcf1c7cbd30661ba2bd4805b256d3` |
| LakeCat's locked Sail source | `querygraph/sail@6471fb9a82620e046d825219eaad26cd569ed91f` |
| LakeCat runtime | locally packaged production ELF; image index `sha256:5f661e70cd67f7c4eb720c2eb030b6373b49a1b7c9b86a25796d98547020ad06` |
| Nessie | `ghcr.io/projectnessie/nessie:0.108.4-java`, index `sha256:c0f42874c810f28ac30fc991e979c1b8cf5a2cbfa94212086cdddeae49629517` |
| Polaris | `apache/polaris:1.5.0`, index `sha256:03a04f0459948da3977f7ea2ad2fb9ea672b2b503ec409c89c2934d400d71c67` |
| Gravitino | `apache/gravitino-iceberg-rest:1.1.0`, index `sha256:906b392c22df95bb3a26085e97a96d2ada3db570c2b40b630f130fa6e1c6648b` |
| MinIO | `minio/minio:RELEASE.2025-09-07T16-13-09Z`, index `sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e` |
| Build runner | `rust:1-bookworm`, observed Rust 1.96.0, image index `sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc` |

The MinIO tag was reconstructed from the published immutable index digest through
the registry. The sibling `boat` compose file used mutable `minio/minio:latest`, so
the digest—not that compose text—is the reliable historical identity.

Tracked raw-evidence integrity was rechecked:

| Artifact | SHA-256 |
| --- | --- |
| `results/commit-2026-08-08-summary.tsv` | `ce0730e6212c087d72fde2983830736e4989b29d3c361f1a00f32ea586b3bdd9` |
| `results/commit-2026-08-08-runs.tsv` | `6aa5cd519aaa2e4c776be360394ea10d5be33ee130d8c7f3cd8b34eec2772819` |
| `results/commit-2026-08-08-object-audit.tsv` | `9cdfb8bbbfef079cd0c934c81308aef1e7bf71bf10dd1e488fba1fd7e494a8c3` |

The summary still yields LakeCat 153.023513 successful concurrent commits/s,
Polaris 129.142092/s, Gravitino 116.884276/s, and Nessie 190.005562/s with 97 HTTP
errors. The first three have five valid measured rounds; Nessie has none. Nessie's
scenario outcome is therefore `fail`, while its successful-request timings remain
diagnostic evidence. `DQ` in the TSV and `Err` in later prose are two labels for
the same validity fact; the v1 historical importer preserves the source field for
audit while the generated matrix now presents one `fail` outcome.

## Reproduction findings

The audit performed read-only or isolated checks before changing source:

1. A detached clean worktree of LakeCat at `3cca8d1c` had the published lockfile
   hash `2c580d644aeaac4bb959b883dd785ddbe81202fb34d1bc5dde5538790ea8e70f`
   and passed:

   ```sh
   cargo check --locked -p lakecat-service --features turso-local,sail-local
   ```

2. A detached clean worktree of catalog-bench at `fbdf684` had the published
   lockfile hash
   `5c9c924c7c0999892c44130d8d2638039c58369b1576b8642c3a5e71de9c8dd6`,
   but Cargo could not load the workspace without an ambient sibling checkout.
   `cache-scan`, `rust-vs-jvm`, and `read-write` used
   `../../../sail/crates/sail-object-store`; the path and Sail revision were not
   represented in the lockfile. This is a clean-checkout reproducibility defect,
   not a benchmark result.

3. The same ambient path explains why the current checkout's
   `cargo test --workspace --locked` wants to rewrite `Cargo.lock`: it resolves
   whichever Sail checkout happens to occupy `../sail`. Phase 0 must replace the
   path with an immutable source revision before claiming standalone reproduction.

   Resolution: `catalog-bench@1aed9f9` pins all three consumers to
   `querygraph/sail@bddb1706ba2308e5029d47f04f03121236edbfa6`; a detached clean
   worktree then passed the full locked workspace suite.

4. Docker Desktop 4.73.0 was launched to run the live protocol, but its engine
   never became available. The backend's last engine failure is `no space left on
   device` inside the Docker VM. The host still had free space; deleting images,
   volumes, or the 476 GiB Docker data store would be destructive and was not
   authorized. Consequently no 2026-08-26 throughput rerun exists. The preserved
   raw evidence and arithmetic reproduce, but the live timing experiment does not.

This is a fully explained discrepancy under the Phase 0 exit rule. It must remain
visible in the first manifest rather than being replaced by guessed numbers.

## Current candidate version pins

These versions are selected for the next profile. They are inputs, not results.
The executable profile in `catalog-bench` is canonical and must carry these exact
revisions and image digests.

| Kind | Component | Selected version and source revision | Container identity or rule |
| --- | --- | --- | --- |
| Catalog | LakeCat | `0.3.0-10-g54ad9dcb`, `54ad9dcb7c941781a77c5210394924a2ae88a459` | Build a stripped production executable from the exact source and record its image digest. |
| Catalog | Apache Polaris | `1.7.0`, `4ac2f059d1cce149453d0a5f1ff1dff980ec97cc` | `apache/polaris:1.7.0`, index `sha256:3495f67f38cca33892a045f7dd3f46eb52387f0fd52d4145538a772fd8aedad7`, Linux ARM64 `sha256:53022013a54121d6f81a130b80df85e2c3c1961c592c39e7e3e2353db2ab7acf` |
| Catalog | Apache Gravitino | `1.3.0`, `40fdf6ab96ac87b47e6d3e14e7c4dc0d815e68f0` | `apache/gravitino-iceberg-rest:1.3.0`, index `sha256:80136ae753ee77735153fc1482a389018f8c2638a54f453cb96967c7194584c7`, Linux ARM64 `sha256:01cf367b77f91652da6c545ad5253d94c11f4e3dd71c5442863eaa330d8a1088` |
| Catalog | Lakekeeper | `v0.13.3`, `12bb82fc0859a82b584afda70e311a0399124a39` | `quay.io/lakekeeper/catalog:v0.13.3`, index `sha256:db2ba6168eb107f22242fb7f2edc4016fa35e57bdcc606894e809c418e32e8dc`, Linux ARM64 `sha256:ba9424131ff088e8eb5263dbdf66e63c2aec0e71687971673ca37a97389394f2` |
| Catalog | Apache Nessie | `0.108.4`, `41d6986725edb95bca176c128300642b8e52d958` | `ghcr.io/projectnessie/nessie:0.108.4-java`, index `sha256:c0f42874c810f28ac30fc991e979c1b8cf5a2cbfa94212086cdddeae49629517`, Linux ARM64 `sha256:10d751690c54c837d687437e1cb269f61b8d2ca541277639d623f495b408fe9c` |
| Client | PyIceberg | `0.11.1`, `8dee48a8e0218353f706133ed035334869a7ee12` | Build/install from the released artifact in a locked client image. |
| Connector | Apache Iceberg Java | `1.11.0`, `6976e020b894f6a6777704df2b8c4458cb291ae9` | Pin each engine-specific runtime JAR and SHA-256. |
| Engine | Apache Spark 3.5 | `3.5.9`, `7c14a3c28b141cc97a330c4d0f5d2a6da7267f85` | `apache/spark:3.5.9`, index `sha256:af02a459c8706e031c835c16f9db3c463816776e2543dd0f828af65606bcf392` |
| Engine | Apache Spark 4.x | `4.1.3`, `77bbf77e86ad48f58b5dfbc6ac882b3e70cf1989` | Selected instead of 4.2.0 because Iceberg 1.11 publishes a maintained Spark 4.1 runtime; image index `sha256:bf9d035a7c32a8ca46aa58d6348182ffd7d2dff6409206ecfbb3915ff1fef211`. |
| Engine | Apache Flink | `2.1.3`, `6cda56b084d5c337b36d2f8ed464bc92093b0a34` | Selected as the newest Flink line with an Iceberg 1.11 runtime; image `flink:2.1.3-scala_2.12-java17`, index `sha256:cc557bbe316d804e83195717a41788dc1ddb9a965887bd0ab83d148480a7802d`. Flink 2.3.0 is a tracked compatibility gap, not silently substituted. |
| Engine | Trino | `483`, `50b0b50b75abd47f830b7805ee1b51716eb4065e` | `trinodb/trino:483`, index `sha256:db58cc93e593a2706553745f276bb119c9810e69918be56ecde088ba7ccb0534` |
| Engine | DuckDB | `1.5.3`, `14eca11bd9d4a0de2ea0f078be588a9c1c5b279c` | Build or package the released CLI/extension set; resolve the final image digest before execution. |
| Object store | MinIO | `RELEASE.2025-10-15T17-29-55Z`, `9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a` | Community MinIO is source-only after this release; build the exact source in Docker and record the resulting digest. Never use `latest`. |
| Shared state backend | PostgreSQL | `17.11` | `postgres:17.11-bookworm`, index `sha256:051f7b7b3abdd564d5d1bd1e8c4b9c1b6e77087d1dd22020ede611c096a272e0`, Linux ARM64 `sha256:b260573038b3770e3d1aba5a9a033bdd19c4b16304b104d4b4256c75d8f15123` |

Catalog-private embedded backends remain part of each catalog identity: LakeCat's
current lock resolves Turso `0.7.0-pre.10`; Gravitino's JDBC/SQLite implementation
and Nessie's configured version store must be recorded from their images. Polaris
and Lakekeeper use separate schemas/databases in the pinned PostgreSQL container;
sharing a PostgreSQL process does not mean sharing catalog state.

Primary release sources are the official [Polaris](https://polaris.apache.org/downloads/),
[Gravitino](https://gravitino.apache.org/downloads/),
[Lakekeeper](https://github.com/lakekeeper/lakekeeper/releases),
[Nessie](https://github.com/projectnessie/nessie/releases),
[PyIceberg](https://github.com/apache/iceberg-python/releases),
[Iceberg](https://iceberg.apache.org/releases/),
[Spark](https://spark.apache.org/news/),
[Flink](https://flink.apache.org/downloads/),
[Trino](https://trino.io/docs/current/release.html), and
[DuckDB](https://github.com/duckdb/duckdb/releases) release records. Registry
digests were resolved directly from each distribution registry on the audit date.

## Apache Ossie audit

The audit pins Apache Ossie commit
[`1d9ebcea2932d3381c0840cc8304f0850d366509`](https://github.com/apache/ossie/tree/1d9ebcea2932d3381c0840cc8304f0850d366509).
Ossie currently has [no published GitHub release](https://github.com/apache/ossie/releases).
Its machine schema is JSON Schema Draft 2020-12 and declares the unreleased
constant `0.2.0.dev0`; filenames and Python types still use the former `osi`
initialism. The integration must therefore record both the commit and schema
artifact hash and remain feature-gated.

Verified upstream surfaces:

- `core-spec/osi-schema.json` is the machine-readable schema;
  `core-spec/spec.yaml` explicitly calls `0.2.0.dev0` a draft.
- `validation/validate.py` checks JSON Schema shape, unique names, relationship
  references, and parseable SQL dialect expressions.
- The 631-line `examples/tpcds_semantic_model.yaml` passes the upstream validator
  at the pinned commit.
- The upstream Python Pydantic model suite passes all 9 tests on Python 3.14.6.
- `custom_extensions` carry an arbitrary vendor name and serialized data string;
  they are preservation content, not trusted policy claims.
- The Polaris converter is a standalone Java 21 REST client. Import maps Polaris
  namespaces/tables to Ossie models/datasets; export creates namespaces and
  physical Iceberg tables. It preserves exact Iceberg physical types in a
  `POLARIS` extension because portable Ossie types lose width, precision and
  nested structure. It is not native semantic-model storage in the Polaris
  server. Native Polaris Ossie support remains an
  [open upstream proposal](https://github.com/apache/polaris/issues/4522).

The upstream validator and Python tests were run successfully. The Polaris Maven
tests were not run: the host has Java 20 and no Maven, while the converter requires
Java 21; the intended Maven-in-Docker fallback was unavailable because of the
Docker VM failure above. This is a recorded test gap, not an inferred pass.

## Known gaps and stale-claim resolution

- **Resolved:** LakeCat's book no longer names the nonexistent
  `catalog-commit-bench` repository or embeds the obsolete LakeCat 0.2.0/Nessie
  0.107.5 table. It links the generated 2026-08-08 matrix and states that the
  bundle is a historical import, not a new live run.
- **Resolved:** `catalog-bench@c0637076` adds versioned neutral scenario, profile,
  manifest, result, evidence, and environment contracts without replacing the
  small process-local `BenchReport`.
- **Resolved:** the raw TSV retains its `DQ` field for immutable audit, while the
  generated matrix uses the closed `fail` outcome and no hand-maintained `Err`
  presentation label.
- The self-contained compose pins historical Polaris 1.5.0 and Gravitino 1.1.0
  and has no Lakekeeper. Those tags remain valid only in the historical profile.
- `boat` uses mutable `minio/minio:latest`. Every future run must use an exact
  source/image digest in the catalog-bench-owned stack.
- The driver documentation still contains stale endpoint and version comments,
  including an old LakeCat `/commit` requirement and a Grust version comment.
- Current performance reports cover one narrow commit operation. They do not
  establish behavioral conformance, multi-engine interoperability, recovery,
  security equivalence, migration, or Ossie support.

These gaps are work items, not hidden exclusions. Their independently verifiable
units are tracked in `BACKLOG.md`.
