# OutcomeHub ownership and compatibility review

Review baseline: `08e9c449cdb9c244a4e9a921b5599f3656ffe3d9`.
This is an implementation-author architecture review, not independent approval.

## Reviewed boundaries

- `collaboration-model` remains storage-free. State domains are still defined once in `contracts`; the new knowledge backend/access enums are generated into the frontend contract.
- The collaboration database transaction implementation remains in `collaboration-store`. Production ownership remains in `collaboration-storage-service`; `collaboration-api` uses `collaboration-client` and never opens the database. Application composition stays in `collaboration-host`.
- The minimal application imports knowledge model/client contracts through service API. It does not link `knowledge-service`, Git/directory implementations, execution workers or optional scenario runtimes. Dependency-aspect tests evaluate actual Bazel edges, including embedded web assets.
- Knowledge management is operator-only, uses the same registry revision fence as other service settings, and forwards to a configured knowledge service. Endpoint and credential values are not supplied by Agent work requests. Targets retain credential environment names only.
- Knowledge writes still begin through the collaboration store's accepted-outcome gate. The API rechecks current acceptance before dispatch. New target preview tokens bind the external revision and exact configuration revision; unknown service results remain unknown until receipt reconciliation.
- Directory writes anchor traversal to open directory descriptors, reject symlinks/special files, bound snapshots and documents, acquire an external writer lock, persist a journal before changes, atomically replace the artifact and persist a receipt. Journal reconciliation reads external content without replaying a write. Non-cooperating file editors do not participate in the lock; baseline checks and content comparison detect observed conflicts but do not provide a multi-writer database transaction over arbitrary editors.
- Git writes use a separate temporary index, preserve the checked-out tree, reject checked-out destination branches, compare-and-swap refs, and never force overwrite a remote branch. Reconciliation uses request identity and durable receipts, never configuration presence or a worker success claim.
- Report/index publication remains a separate reviewed adapter flow. Its origin and layout are now explicit configuration. Path allowlists, full reviewed snapshots, source/prompt/policy/knowledge evidence and validation checks remain mandatory.

## Compatibility evidence

- Frozen API serialization fixtures and historical digest fixtures are retained unchanged. Package paths, source aliases and test locations are migrated around them.
- Canonical HTTP routes use the same handlers as old paths. A shared route identity mapping preserves Agent/operator classification. Regression tests exercise unauthenticated, operator and malformed Agent requests on both forms.
- Existing commands remain real executable entrypoints using the same source; canonical names are separately packaged. No data directories or serialized state values are renamed.
- Existing tests continue to cover execution attempt fences, stale results, membership revocation, core startup without scenario configuration, frontend localization, and embedded/remote storage parity.
- Added adapter tests cover both backends, content preservation, conflicts, target revisions, read-only configuration, credential rejection, symlink escape and interrupted publication recovery. RPC tests verify management through the same protocol used by both user surfaces.

Final Bazel and full Qualitygate results are delivered separately and must bind the final snapshot. This review does not replace those checks.
