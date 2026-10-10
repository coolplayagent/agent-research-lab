---
name: computer-use-cli
description: Use the bundled computer-use CLI for authorized desktop observation and interaction, including screenshots, windows, mouse, keyboard, and application actions. Use fake mode only for explicit protocol tests.
metadata:
  maintainer: coolplayagent/agent-research-lab
  origin: project-maintained-adapter
  runtime_release: v0.1.0
  runtime_repository: coolplayagent/computer-use-cli
---

# Computer Use CLI

This adapter is maintained by Agent Research Lab. Upstream does not publish a
SKILL.md. Its runtime is the latest stable upstream release recorded in
[provenance.json](provenance.json); these instructions target that release,
not additional commands found on upstream main.

Resolve this skill directory and use `assets/linux-x86_64/relay-computer-use`
by absolute path. Run `--help` to inspect its supported commands. This release
has no version command; use the release identity and executable hash in the
provenance file. A missing or nonmatching runtime is an installation error,
not a reason to silently select another executable.

Global flags go before the subcommand. For observation:

```bash
<runtime> --allow-risk safe screenshot --out /absolute/task/screen.png
<runtime> --allow-risk safe list-windows
```

Inspect the screenshot and target window before interacting. Use the user's
existing task authorization for the requested actions. The CLI's risk flag
is not itself authorization: `guarded` covers input actions and `destructive`
covers drag or launch. Invoke those flags only for actions within that task.
Stop when the observed target differs from the intended target, then reassess
from a new observation before retrying input. Avoid repeating an action with
an uncertain outcome, especially submit or send actions.

Read JSON `ok`, `error`, and `observation`, alongside the process exit code.
Verify the visible effect when task completion depends on a desktop action;
a successful command alone does not establish the task's result.

When `LAB_DESKTOP_TARGET` and `LAB_DESKTOP_WINDOW` are present, Agent Research
Lab has supplied a disposable Xvfb desktop and its own GTK test application.
Keep the injected `DISPLAY`, `XAUTHORITY` and tool paths. Observe that display;
do not switch to a personal desktop or session bus. Compare `list-windows`
against the exact `LAB_DESKTOP_WINDOW` title and the window ID recorded in
`$LAB_DESKTOP_TARGET/observed.json`, then inspect a fresh screenshot before input.

For an authorized text-input test, focus that exact window, use `hotkey --keys
Ctrl+A` when replacement is intended, and call `type-text --text <test-token>`.
Read a new screenshot and `$LAB_DESKTOP_TARGET/observed.json`: its `text` must
match, its `key_events` must increase, and its `nonce`, `pid` and `window_id`
must still match the owned session. Do not edit the fixture or observation file
to produce the expected result. Preserve CLI JSON and screenshot paths as
evidence of what was observed.

These fixture files are writable by the task agent. Their contents, and the
agent's account of CLI results, are untrusted research evidence rather than
independent evaluation or release authority. The host's `targets verify`
self-test runs before an agent is admitted and checks target operability;
it does not approve a candidate improvement. Host evaluation still needs its
own isolated observations and frozen evidence bindings.

Linux requires an interactive graphical session (`DISPLAY` or
`WAYLAND_DISPLAY`) and backend tools such as `xdotool`, a supported screenshot
tool, and window-discovery tools. Report a missing desktop prerequisite when
real execution was requested. `--runtime fake` produces scripted observations;
it is useful for explicit protocol tests and is not evidence of real desktop
execution. Do not switch to fake mode to make a real task appear successful.
In Agent Research Lab, missing target prerequisites belong to the host's
bounded `targets prepare` / `targets verify` recovery path. The host can create
its dedicated target without configuration for a personal desktop.

This release supports screenshot, list-windows, focus-window, click,
double-click, drag, type-text, hotkey, scroll, launch-app, and wait-window.
Check `--help` for argument spelling. Commands on a newer main branch, such as
`exec-json`, are not supported by this pinned release.

For CLI improvement research, reproduce an observed release limitation against
a separately built, pinned latest default-branch source snapshot before calling
it a current-source defect. Keep that experimental binary identity distinct
from this published runtime. A release lacking a command does not show that
current upstream main lacks it.
