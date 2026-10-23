# Native runtime file budget review

This is an author review, not independent approval. Two isolated replays of the
failing research worker terminated with SIGXFSZ. A private syscall trace identified
the exact write: the native Codex process attempted to persist a 41,304,532-byte
remote plugin catalog in its disposable home. The 16 MiB RLIMIT_FSIZE truncated
that write and the next write raised EFBIG/SIGXFSZ. Short validation tasks often
finished before this asynchronous catalog refresh, explaining their success.

The shared runtime single-file budget is now 64 MiB on Linux and Windows.
Protocol stdout remains limited to 256 KiB and diagnostic stderr to 4 MiB.
Windows retains its aggregate 256 MiB and 10,000-entry writable-state limits.
Linux retains OS file, CPU, address-space and descriptor limits, read-only runtime
mounts, private namespaces and bounded execution lifetime. Each run still owns
and deletes its own writable home/workspace; no shared writable cache is added.

The bridge now reports the native ExitStatus in private diagnostics, including
Unix signals, rather than describing every nonnumeric exit as a login/config
problem. No private output is returned in task errors. Success still requires a
valid completed protocol response, exact receipt digests and the existing host
artifact/acceptance gates. A larger runtime-file allowance does not increase
accepted task output size or promote failed executions.

Regression coverage writes a catalog matching the observed size, rejects files
above 64 MiB, retains the existing stdout/stderr bounds, verifies disposal and
checks Windows aggregate quota accounting. A signal regression preserves SIGXFSZ
in private native diagnostics. Real research execution and full repository tests
plus Qualitygate remain required before delivery.
