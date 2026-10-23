# Execution configuration refresh review

This is an author review, not independent approval. The live failure occurred
before a coordinator claim: a successful worker revalidation replaced the shared
bridge path, leaving the coordinator pinned to the previous adapter digest.
Other workers' evidence correctly became invalid for the changed adapter.

- Registry saves now update the selected coordinator's adapter digest and clear
  stale protocol evidence atomically. Existing persisted selections are repaired
  on coordinator configuration access. Adapter identity and explicit model stay
  selected; missing or disabled adapters never fall back to another adapter.
- Selection repair is not compatibility evidence. The scheduler still requires
  the real coordinator protocol probe before claiming work. An old probe cannot
  commit across a changed configuration fingerprint. Worker bindings still need
  their own current validation evidence, and changed adapters discard old proofs.
- A relocated packaged bridge may reuse its saved path only when both executable
  digests match. Changed or missing bridges select the packaged binary and cause
  normal evidence invalidation. Every requested validation still executes its
  real challenge; this does not manufacture evidence for other members.
- Adapter selection failures now retain the specific private reason. Public
  error codes, serialized states, task leases, budgets, storage ownership and
  acceptance gates are unchanged.

Coverage includes registry save/restart repair, late old-probe rejection,
disabled adapters, changed executor endpoints, bridge identity comparisons and
private worker diagnostics. Full repository tests and Qualitygate are required
on the final source snapshot; live revalidation remains necessary for affected
bindings and for the coordinator protocol.
