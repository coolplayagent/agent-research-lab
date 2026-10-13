# Responsive workspace and directory editing review

Agent-authored source and behavior review; not independent approval or research
acceptance. This change addresses layout and missing presentation interactions.

## Ownership and invariants

- Rust APIs, serialized lifecycle values and storage ownership are unchanged.
  Frontend state checks reuse the generated shared contracts. No state dictionary,
  storage migration, credential handling or adapter execution path is introduced.
- Renaming a digital person preserves the stable ID and existing application ID.
  Identity cache updates notify messages, mentions and the collaboration view.
- Topic edits use the server's saved revision and identity. Group ownership stays
  immutable. Request generations fence delayed topic and subject responses; a
  closed detail view cannot be reopened by an older response.
- Archived topic controls reflect existing backend authority. Version nodes and
  parent edges remain append-only. An unlinked node remains unassessed, and worker
  submissions do not grant host acceptance. Existing research evidence gates and
  attempt fencing are unchanged.
- Settings mutations still submit complete versioned configuration through the
  service API. Deleting a referenced adapter is rejected. Storage selection does
  not replace a running store. Reachability and disabled configuration are not
  represented as actual model-task success.
- Ordinary group membership refreshes picker exclusions. Temporary conversation
  expansion still forks a new fixed audience; history inclusion is explicit and
  the source conversation remains intact. Loading another member list clears
  obsolete controls before the response arrives.

## Verification scope

`layout_tests` exercises out-of-order directory/detail responses, saved topic
identity, archived controls, application-preserving rename, remove/re-add member
selection and mutually exclusive mobile drawers. Existing workspace, locale,
wire-compatibility and state-machine tests remain required through Bazel.

Browser scenarios use a bounded standalone core with a separate development store,
without research configuration. They cover create/edit topic and subject, append
an unassessed version, identity creation/rename, services/adapters/bindings,
configuration rejection, dirty-form dismissal, goal creation/cancellation,
messaging/reply, group membership, archive/restore, fixed-audience expansion,
conversion and boards. Layout checks cover nine routes at seven viewport sizes,
and nine dialog types at five sizes including phone portrait and short landscape.
Screenshots and raw runs remain outside Git. Final delivery requires the packaged
assets, the full repository test suite and full snapshot-bound Qualitygate.

These checks establish behavior for the exercised browser, viewports and fixture
shapes. They are not exhaustive accessibility certification, vendor-adapter
compatibility evidence, or a new accepted research result.
