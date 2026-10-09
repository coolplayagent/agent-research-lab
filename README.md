# Agent Research Lab

Rust + Bazel runtime for reproducible AI-SDLC and long-horizon agent research.
Research knowledge belongs to [SuperPOD](https://github.com/stevetdp/superpod).

The runtime integrates workflow-cli, relay-knowledge, into-markdown, qualitygate-cli,
computer-use-cli, relay-memory and repo-sandbox. Model roles and machine paths are
configured explicitly; credentials and experiment data are not stored in Git.

```sh
bazel build //:agent-research-lab
bazel test //...
```

Linux is the first supported runtime platform. Toolchains and dependencies are pinned.
