# Evolution feature retirement review

This is an implementation-owner semantic review, not independent approval.
The user explicitly expanded removal to include the research CLI integration and
historical feature evidence. This supersedes retention of collaboration version
nodes for this retirement; unrelated research admission and promotion gates remain.

## Ownership and lifecycle

- Both web hosts stop creating the iteration scheduler. The iteration module,
  scenario reconciliation hook, software-evolution adapter, research version sync,
  activation CLI commands and software-delivery crate are removed together.
- The collaboration model, client, storage service and writer no longer expose
  graph or iteration operations. Topic/subject revisions and group ownership remain.
- Schema 9 is required before storage starts. There is no online database upgrade
  or read of the storage owner's live database by another service.
- Offline migration selects every iteration binding, including older rounds and
  terminal runs, plus software-evolution goals. It erases their results, paths,
  owned outcomes and version/iteration tables. Removing these goals prevents the
  ordinary executor from resuming orphaned work after scheduler removal.
- System-message bodies and event payloads for erased goals become tombstones;
  sequence, sender, request identity, reply links and reader cursors remain intact.
  The preservation digest normalizes only those selected payload fields and
  excludes only explicitly erased rows. Ordinary goals/chat retain byte checks.
- Independent outcomes referencing retired outcomes stop migration instead of
  rewriting their immutable input/digest or silently cascading to unrelated work.
  Foreign-key/integrity checks run before publishing the prepared copy.
- A previous migration manifest may be replaced on a copied older-schema store.
  The source inventory still binds its original bytes; current migrated stores
  reject duplicate migration. Rollback requires the original data and binaries.

## Presentation and verification

The three pages, their modules/assets, navigation/actions, metric renderer and
refresh handlers are removed. Old routes fall back to groups or digital-person
management. Binding log selection, adapter testing and automatic runtime preparation
remain owned by their existing modules.

Regression evidence covers retired HTTP/assets/CLI commands, old-route fallback,
remaining topic revision/archival behavior, API wire fixtures, real storage RPC,
nonempty migration with multiple rounds/results/outcomes, preserved chat replies,
blocked cross-feature references, all historical schema upgrades and repeated
migration. The final snapshot also requires full Bazel and Qualitygate checks,
Linux/Windows CI and packaged browser/deployment verification.
