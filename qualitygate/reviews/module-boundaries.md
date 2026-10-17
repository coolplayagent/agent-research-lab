# Collaboration module boundary review

Baseline: `4fd42de092b8af96913c7df64de0c0b82e9123d0`.
This is a local implementation/semantic review, not independent signed approval.
Final acceptance additionally requires full snapshot-bound Qualitygate.

## Ownership and dependency direction

- `collaboration-model` owns transportable data and deterministic validation.
  Shared lifecycle enums still come from `contracts`; no state values or frontend
  labels were redefined. Model SHA-256 output retains the existing byte algorithm.
- `service-api` no longer imports the database, storage service or filesystem
  persistence helpers. Host registry persistence/probes moved to `service-registry`.
- `im-client` owns the request/client contract and stream interfaces. Its embedded
  port accepts owned requests and boxed streams, not database handles. Production
  calls use the existing private Unix-socket protocol. Local calls are serialized
  through the same request/result types and bounded like remote requests.
- `im-storage` alone implements request dispatch against `crystal::Hub`. Dispatch,
  authorization and owner connection/serve logic are byte-identical to baseline.
  The embedded implementation is provided explicitly by `im-host` using the
  existing separate development directory; a configured production endpoint never
  falls back to local storage after an error. Registry restart requirements remain.
- `crystal` retains the transaction owner for messages, goals, outcomes and
  coordination metadata. Splitting these transactions would endanger authorization,
  attempt fencing and host acceptance. Its old type exports remain compatibility
  re-exports of the single model definitions.
- Executor, sandbox, Codex and outcome adapters have independent packages and
  depend on shared service contracts. The legacy worker launcher only composes the
  generic executor and sandbox. Core HTTP/application targets exclude these adapters.
- `apps/ai-im` owns the minimal command entrypoint. `apps/research-cli` owns research
  dispatch; `apps/research-console` owns its UI and scenario integration. Software
  activation and verification parameters have separate library owners. `app/` is
  an explicit compatibility facade, not an alternative implementation.

## Behavior and compatibility

HTTP paths, RPC operations, database schema, source/prompt/policy pins, SuperPOD
research requirements and goal/outcome acceptance state machines are unchanged.
SuperPOD remains optional in core outcome collaboration. Existing external Git
knowledge write, CAS, idempotency and receipt reconciliation tests moved intact.
No automatic knowledge push/merge or promotion was added.

The protocol fixture executes original baseline serializers for 16 representative
cases, including omitted fields, nested outcome requests, lineage and service
configuration. Both client transports test durable delivery, duplicate/conflicting
request IDs, event delivery, preserved Unicode and membership revocation. Existing
attempt/claim/verification and research gate regression suites remain enabled.

Console generated JavaScript assets are byte-identical after relocation; TypeScript
reproducibility and locale/boundary checks use the new asset paths. Historical
Bazel executable labels and output paths compile the same new entrypoint sources.
Core, adapter and full packages are all checked by CI and Qualitygate.

Bazel analysis tests traverse actual transitive dependencies and package contents,
including negative boundary assertions for research, database and vendor adapter
imports. Standalone application tests start and restart the real binary without
research config or external services and verify bounded clean exit. These tests
complement the ownership review; they do not certify external vendor/model services.
