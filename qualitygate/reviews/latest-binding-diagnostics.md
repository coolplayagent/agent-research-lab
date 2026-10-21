# Latest binding process and private execution diagnostics review

This is an agent-authored source review, not independent approval.

The registry remains the sole owner of binding configuration, evidence and the
binding process journal. Each person has at most one process record. Replacing a
terminal process atomically persists its request fingerprint and the new process
before deleting obsolete diagnostic files. Startup accepts legacy arrays, chooses
the newest record deterministically, reconciles interrupted work against the
existing configuration evidence, then removes obsolete logs. Failed journal writes
restore the previous in-memory process. Cleanup failures mark the new process as
failed rather than leaving a running process with no worker. Late completion of a
removed process cannot recreate its diagnostic or overwrite the current record.

A running process still blocks a second process for the same person, and global
validation concurrency remains four. Fingerprints retain the existing bounded
request-history guarantee (256 obsolete requests); they contain no prompts,
program paths, diagnostic contents or credentials. New request IDs are mandatory
for intentional retries. The existing configuration revision and cancellation
checks, challenge receipt, request identity, executable/configuration/adapter
pins and atomic configuration/evidence commit remain the acceptance boundary.
A worker's success claim alone still cannot bind a person.

Only private service protocol replies carry bounded execution details. Error
Display, operation APIs and HTTP errors exclude those details. Sandbox stderr is
bounded, copied credentials and environment values are redacted, and URLs are
removed before it leaves the disposable sandbox. Registry diagnostics are private
and removed when superseded; sandbox services do not keep another raw log copy.
Network routing is passed by approved environment names only. The optional Codex
native code-mode host is mounted read-only and included in executable identity.
Its V8 address reservation requires a 16 GiB Linux virtual-address ceiling; the
wall-time, output, file and disposable mount limits remain enforced. Virtual
address limits do not establish a resident-memory budget.

Regression evidence covers legacy migration, replacement, stale request retries,
restart recovery, concurrent people, failed persistence, late completion,
redaction, private protocol transport, HTTP exclusion, generated contracts,
frontend rendering and packaged binding transitions with deterministic peers.
Live model compatibility is additionally checked in separate disposable state;
it does not authorize a production deployment or establish research promotion.
