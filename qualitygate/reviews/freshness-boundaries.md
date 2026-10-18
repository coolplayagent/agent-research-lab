# Source and skill freshness boundary review

Baseline: `10a3b6bbe11ff9fb32ad3793faafe367a269ab56`.
This is local implementation and semantic review, not independent signed approval.
Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

| Module | Responsibility |
| --- | --- |
| `protocol` | Stable retained remote, skill, manifest and source-proof records. |
| `identity` | Authorized GitHub identities, origin parsing, immutable hex bindings, branch validation and URL encoding. |
| `client` | Configurable Git/GitHub paths and bounded process capture, exit and JSON checks. |
| `remote` | Actual default-branch capture, dedicated host fetch ref and fresh binding verification. |
| `releases` | Stable published release lookup and verified no-release interpretation. |
| `skills` | Manifest-wide admission, per-skill byte/provenance verification and local attempt checks. |
| `files` | Canonical skill-tree digest, bounded regular-file hashing and symlink-component rejection. |
| `source_gate` | Hashed exact-commit full gate proof for a host source installation. |

The root preserves every public record, function and RemoteClient method. Its
public configuration fields and default timeout remain unchanged. Wire records
and identity validation do not invoke process transport. Remote/release/skill
operations share the bounded client; local byte verification needs no network.
Source proof verification only reads retained evidence. No core collaboration,
storage ownership, vendor adapter or mandatory SuperPOD dependency is introduced.

## External state domains

`QualitygateProfile`, `QualitygateScope`, `QualitygateDecision` and
`QualitygateSnapshotMode` are separate shared contracts. Their explicit known wire
values cover quick/full; repository/delivery/task/path; pass/fail/incomplete;
and diff/staged/worktree/mr. Profile and selector spellings were checked against
the installed CLI; its decision schema documents the incomplete outcome. Unknown,
missing or incorrectly typed values parse to no recognized state, never success.
The binary independent-evaluation `VerificationVerdict` remains a separate domain.

Both source-skill and release-delivery evidence now parse the same shared domains
at their boundary. This changes representation, not their accepted report sets:
source overrides still accept only delivery/task scope and exact diff mode;
delivery still accepts its historical repository scope too. A common parser does
not merge these authorization policies. Existing artifact JSON and error messages
remain unchanged. TypeScript is regenerated through Bazel, and Chinese labels
reside only in the locale catalog. The only new dependency is the existing local
contracts crate; no external version changes.

## Semantic review

- A remote snapshot still resolves the origin to an authorized repository, asks
  GitHub for its actual default branch and commit, validates a dedicated ref,
  fetches that exact branch without tags/FETCH_HEAD changes, compares the fetched
  commit, then rechecks remote binding. The feature branch, index and user work
  remain untouched. A branch-name switch invalidates a binding even at the same
  commit; checked timestamps are not identity. Fetch/query failure has no fallback.
- A no-release result still requires a failed command, structured string/numeric
  404 and an HTTP 404 diagnostic, followed by a successful matching repository
  lookup. Other failure, malformed JSON, mismatched identity, draft/prerelease,
  missing/nonpositive ID or empty tag does not mean usable or absent release.
- Manifest version, nonemptiness, safe names and uniqueness are checked before
  parallel verification. Every item validates repository/digest/timestamp fields,
  absolute paths, SKILL.md, canonical tree bytes and runtime bytes before remote
  provenance. Only a complete successful map returns a refreshed manifest; the
  supplied manifest is never rewritten by this API.
- A release installation binds exactly one nonempty tag and positive ID, forbids
  a source override and must equal the current release. A source installation
  binds only a source commit, requires full proof when a release exists or proof
  is supplied, then compares with the current remote default commit. Partial or
  mixed source/release identities remain invalid.
- Source proof keeps absolute/no-link paths, O_NOFOLLOW/O_NONBLOCK open, regular
  file and 16 MiB limits, and the same bounded bytes for hashing and parsing. It
  requires full profile, delivery/task scope, complete pass, an explicit empty
  pending-check array, nonempty delivery and diff head equal to the source commit.
  A changed artifact hash fails before any report authority is considered.
- Local attempt verification rehashes both tree and runtime. Tree identity retains
  sorted relative POSIX UTF-8 paths and path/NUL/raw-file-hash/newline framing;
  empty directories do not contribute. The 10,000-entry, 1 GiB aggregate and 256 MiB
  per-file limits, size-change check, parent traversal and symlink/special-file
  rejection remain unchanged. Moving these functions adds no new race guarantee.
- Delivery evidence keeps independent evaluator identity, upstream, source,
  prompt, policy, SuperPOD, skills, content and verification digest gates. Only
  its three Qualitygate category comparisons change to typed parsing; existing
  accepted ranges and all subsequent checks remain intact.

## Evidence and limits

The extraction audit compares 36 definitions/implementations/methods across
freshness and delivery evidence, every public freshness export and two fixture
helpers. It normalizes private visibility/formatting and the exact equivalent
shared-state predicates at both report boundaries. All eleven original tests in
the affected freshness/evidence/contracts files retain their bodies. Lockfile
comparison checks one local dependency edge and the two regenerated Bazel input
hashes, with no external changes.

Nine new regressions cover verified release absence, stable release shape,
malformed remote bindings, every source-proof condition, legacy delivery scopes,
shared state parsing, failed multi-skill verification without partial return or
input writes, hashing bounds/paths, and Unicode wire data in an external fixture.
Focused checks pass twelve freshness, fifteen delivery and five contracts tests,
plus locale integrity. Full repository checks cover generated contract drift,
consumers, packaging, retained research gates and server/presentation boundaries.

Deterministic local Git/CLI fixtures establish tested semantics; they do not
prove current production repositories, installed tools or real release authority.
Existing host permissions and process isolation remain necessary. No live remote
publication, installation, candidate promotion or deployment occurs in this change.
