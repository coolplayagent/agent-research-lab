# Legacy retirement ownership review

Baseline: `174dbd8b22528977d47b10cbb3db725ab47d8671`.
This is an implementation-author review, not independent approval.

The offline `apps/migrate` binary owns historical SQLite upgrades and persona/completion conversion. Core application and storage services do not depend on that binary. The production database still has one owner; migration requires stopped services and writes only to a separate destination. The current SQL schema is created directly, and unsupported existing versions are rejected before changing the database.

Completion records are defined once in execution-model; lifecycle enums remain in contracts. Frozen jobs and receipts retain their bytes and policy/source/prompt/knowledge bindings. Missing historical side-effect evidence becomes Unknown with blocked followups, never successful acceptance or automatic replay. Ordinary optional v1 configuration fields and immutable evidence serialization remain current contracts, not mutable startup migration paths.

HTTP handlers and authorization now use canonical paths directly. Removed paths have no registered routes. Unknown resources remain subject to the origin boundary and return 404. Existing operator and Agent tests exercise the canonical endpoints, and explicit negative tests cover retired endpoints.

The repository no longer vendors skills or research knowledge. New reviewed insights come from a committed regular blob in the already bound external knowledge repository, with the same revision and digest checks. No missing-file fallback, external publication, or evidence gate reduction is introduced.

Executable dependency-aspect and frozen wire tests moved from the root architecture directory to verification. Full Bazel and Qualitygate reports must bind the final snapshot; text searches and this review do not replace those checks.
