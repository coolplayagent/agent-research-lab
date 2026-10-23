"""Exercise the actual packaged local runtime using deterministic vendor peers."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request
import urllib.error


def run(archive, fixture):
    with tempfile.TemporaryDirectory(prefix="oh-runtime-", dir=os.environ.get("TEST_TMPDIR")) as temporary, tempfile.TemporaryDirectory(prefix="oh-rs-", dir="/tmp") as state_temp:
        root = Path(temporary)
        with tarfile.open(archive) as package:
            package.extractall(root, filter="data")
        binaries = root / "outcome-hub"
        for name in ("outcome-hub", "outcome-hub-services", "outcome-hub-agent", "outcome-hub-storage", "outcome-hub-knowledge"):
            assert (binaries / name).is_file(), name
        peer = root / "fixture-agent"
        shutil.copyfile(fixture, peer)
        peer.chmod(0o700)
        with socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        state = Path(state_temp)
        isolated = root / "profile"
        isolated.mkdir(mode=0o700)
        environment = {"PATH": "/usr/bin:/bin", "HOME": str(isolated), "TMPDIR": str(root)}
        def start():
            with (root / "host.log").open("ab") as log:
                return subprocess.Popen([str(binaries / "outcome-hub"), "serve", "--state-dir", str(state), "--listen", f"127.0.0.1:{port}", "--max-seconds", "180"], env=environment, stdout=log, stderr=log)
        host = start()
        def api(path, body=None, intent="manage-services"):
            request = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers={"Content-Type":"application/json", "Origin":base, "X-Crystal-Intent":intent})
            with urllib.request.urlopen(request, timeout=40) as response:
                return json.load(response)
        def children():
            output = subprocess.run(["ps", "-o", "pid=", "--ppid", str(host.pid)], capture_output=True, text=True)
            return {int(pid) for pid in output.stdout.split()}
        def wait_ready():
            for _ in range(80):
                assert host.poll() is None, (root / "host.log").read_text()
                try:
                    api("/api/operator/session")
                    return
                except OSError:
                    time.sleep(0.1)
            raise AssertionError("Host did not become ready")
        def wait_removed(pids):
            for _ in range(50):
                if all(not Path(f"/proc/{pid}").exists() for pid in pids):
                    return
                time.sleep(.1)
            raise AssertionError("Owned workers survived cleanup")
        owned = set()
        try:
            wait_ready()
            view = api("/api/v1/collaboration/services")
            assert view["active_storage"][0] == "storage"
            assert len(view["configuration"]["services"]) == 3
            storage_pids = children()
            assert len(storage_pids) == 1, "Only durable storage starts before binding"
            assert not (state / "crystal").exists()
            assert (state / "private/managed-storage/messages.sqlite").is_file()
            assert next(s for s in view["local_runtime"] if s["component"] == "storage")["state"] == "ready"
            api("/api/v1/messaging/manage", {"operation":"person", "person":{"id":"persistent", "name":"Persistent"}}, "manage-crystal")
            # Recovery reopens the same durable store, without replacing configuration.
            import signal
            os.kill(next(iter(storage_pids)), signal.SIGKILL)
            for _ in range(100):
                try:
                    recovered = api("/api/v1/collaboration/people")
                    if any(p["id"] == "persistent" for p in recovered["people"]): break
                except OSError: pass
                time.sleep(.1)
            else: raise AssertionError("Storage did not recover persisted collaboration data")
            storage_pids = {pid for pid in children() if Path(f"/proc/{pid}/exe").resolve().name == "outcome-hub-storage"}
            assert len(storage_pids) == 1

            worker = binaries / "outcome-hub-services"
            worker.rename(binaries / "missing-worker")
            try:
                api("/api/v1/collaboration/services/prepare", {"revision":view["configuration"]["revision"]})
                raise AssertionError("Missing worker must fail preparation")
            except urllib.error.HTTPError as error:
                assert error.code == 503
                assert json.load(error)["error"]["code"] == "runtime_package_missing"
            finally:
                (binaries / "missing-worker").rename(worker)
            assert api("/api/v1/collaboration/services")["configuration"] == view["configuration"]
            changed = json.loads(json.dumps(view["configuration"]))
            executor = next(s for s in changed["services"] if s["kind"] == "executor")
            executor["kind"] = "sandbox"
            executor["sandbox_id"] = None
            try:
                api("/api/v1/collaboration/services", changed)
                raise AssertionError("Existing service role must remain fixed")
            except urllib.error.HTTPError as error:
                assert json.load(error)["error"]["code"] == "service_kind_immutable"
            for _ in range(2):
                ready = api("/api/v1/collaboration/services/prepare", {"revision":view["configuration"]["revision"]})
                assert all(h["status"] == "healthy" for h in ready["health"]), ready["health"]
                current = children() - storage_pids
                assert len(current) == 2, current
                if owned:
                    assert owned == current, "Preparation must reuse owned workers"
                owned = current
            # Adapter testing works without creating or selecting a digital person.
            people_before = api("/api/v1/collaboration/people")
            test_request = {"request_id":"adapter-only", "adapter_id":"codex", "revision":view["configuration"]["revision"], "program":str(peer), "model":"fixture/saved-model"}
            api("/api/v1/collaboration/adapter-tests", test_request)
            deadline = time.monotonic() + 30
            while True:
                tested = api("/api/v1/collaboration/adapter-tests/adapter-only")
                if tested["state"] != "running": break
                assert time.monotonic() < deadline
                time.sleep(.1)
            assert tested["state"] == "succeeded", tested
            assert all(step["state"] == "passed" for step in tested["steps"])
            assert api("/api/v1/collaboration/people") == people_before
            view = api("/api/v1/collaboration/services")
            assert not view["configuration"]["bindings"]
            assert not view["configuration"].get("binding_evidence")
            assert view["configuration"]["adapter_test_evidence"]["codex"]["request_id"] == "adapter-only"
            assert next(a for a in view["configuration"]["adapters"] if a["id"] == "codex")["enabled"]
            assert api("/api/v1/collaboration/adapter-tests", test_request)["state"] == "succeeded"
            assert api("/api/v1/collaboration/bindings") == []
            def wait_test(identifier, executing=False):
                deadline = time.monotonic() + 15
                while True:
                    record = api("/api/v1/collaboration/adapter-tests/" + identifier)
                    if record["state"] != "running" or (executing and any(s["stage"] == "execute" and s["state"] == "running" for s in record["steps"])):
                        return record
                    assert time.monotonic() < deadline, record
                    time.sleep(.02)
            # Cancellation during execution must preserve both settings and prior evidence.
            before = view["configuration"]
            hang = root / "hang-agent"
            shutil.copyfile(peer, hang); hang.chmod(0o700)
            api("/api/v1/collaboration/adapter-tests", {**test_request, "request_id":"adapter-cancel", "revision":before["revision"], "program":str(hang)})
            assert wait_test("adapter-cancel", executing=True)["state"] == "running"
            api("/api/v1/collaboration/adapter-tests/adapter-cancel/cancel", {})
            assert wait_test("adapter-cancel")["state"] == "cancelled"
            assert api("/api/v1/collaboration/services")["configuration"] == before
            # A successful task cannot overwrite settings saved while it executes.
            delay = root / "delay-agent"
            shutil.copyfile(peer, delay); delay.chmod(0o700)
            api("/api/v1/collaboration/adapter-tests", {**test_request, "request_id":"adapter-conflict", "revision":before["revision"], "program":str(delay)})
            assert wait_test("adapter-conflict", executing=True)["state"] == "running"
            api("/api/v1/collaboration/services", before)
            concurrent = api("/api/v1/collaboration/services")["configuration"]
            conflict = wait_test("adapter-conflict")
            assert conflict["state"] == "failed" and conflict["diagnostic"]["code"] == "revision_conflict", conflict
            assert api("/api/v1/collaboration/services")["configuration"] == concurrent
            assert api("/api/v1/collaboration/people") == people_before
            assert api("/api/v1/collaboration/bindings") == []
            for agent in ("codex", "cc", "hds", "pi"):
                person = "fixture-" + agent
                api("/api/v1/messaging/manage", {"operation":"person", "person":{"id":person, "name":person}}, "manage-crystal")
                config = api("/api/v1/collaboration/services")["configuration"]
                request = {"request_id":"verify-"+agent,"person_id":person,"adapter_id":agent,"revision":config["revision"],"program":None if agent == "codex" else str(peer),"model":"fixture/deterministic" if agent == "hds" else None}
                api("/api/v1/collaboration/bindings", request)
                deadline = time.monotonic() + 30
                while True:
                    operation = api("/api/v1/collaboration/bindings/"+request["request_id"])
                    if operation["state"] != "running":
                        break
                    assert time.monotonic() < deadline, operation
                    time.sleep(0.1)
                if operation["state"] != "succeeded":
                    for diagnostic in state.glob("private/**/*.diagnostic.json"):
                        print(diagnostic.read_text(), file=sys.stderr)
                assert operation["state"] == "succeeded", operation
                saved = api("/api/v1/collaboration/services")["configuration"]
                if agent == "codex":
                    command = next(a for a in saved["adapters"] if a["id"] == agent)["command"]
                    assert command[command.index("--model") + 1] == "fixture/saved-model"
                    assert operation["program"]["executable"] == str(peer)
                assert saved["bindings"][person] == agent
                assert saved["binding_evidence"][person]["request_id"] == request["request_id"]
                assert api("/api/v1/collaboration/bindings", request)["state"] == "succeeded"
            # A failed change preserves the active binding, but replaces its process log.
            config = api("/api/v1/collaboration/services")["configuration"]
            failed_request = {"request_id":"missing-cc", "person_id":"fixture-codex", "adapter_id":"cc", "revision":config["revision"], "program":str(root / "missing-agent")}
            api("/api/v1/collaboration/bindings", failed_request)
            deadline = time.monotonic() + 15
            while True:
                failed = api("/api/v1/collaboration/bindings/missing-cc")
                if failed["state"] != "running": break
                assert time.monotonic() < deadline
                time.sleep(.1)
            assert failed["state"] == "failed"
            assert api("/api/v1/collaboration/services")["configuration"] == config
            old_log = state / "private/binding-missing-cc.diagnostic.json"
            assert old_log.exists()
            request = {"request_id":"replacement-codex", "person_id":"fixture-codex", "adapter_id":"codex", "revision":config["revision"], "program":str(peer)}
            api("/api/v1/collaboration/bindings", request)
            deadline = time.monotonic() + 30
            while True:
                operation = api("/api/v1/collaboration/bindings/replacement-codex")
                if operation["state"] != "running": break
                assert time.monotonic() < deadline
                time.sleep(.1)
            assert operation["state"] == "succeeded", operation
            latest = api("/api/v1/collaboration/bindings")
            assert len([record for record in latest if record["request"]["person_id"] == "fixture-codex"]) == 1
            assert not old_log.exists()
            for path, body in [("/api/v1/collaboration/bindings", failed_request), ("/api/v1/collaboration/bindings/missing-cc", None)]:
                try:
                    api(path, body)
                    raise AssertionError("An obsolete request must not execute again")
                except urllib.error.HTTPError as error:
                    assert json.load(error)["error"]["code"] == "superseded"
            saved = api("/api/v1/collaboration/services")["configuration"]
            assert saved["binding_evidence"]["fixture-codex"]["request_id"] == "replacement-codex"
            host.terminate()
            host.wait(timeout=12)
            wait_removed(owned)
            host = start()
            wait_ready()
            restored = api("/api/v1/collaboration/services")
            assert restored["configuration"] == saved, "Restart must preserve configuration and evidence"
            assert all(h["status"] == "healthy" for h in restored["health"]), restored["health"]
            storage_pids = {pid for pid in children() if b"outcome-hub-storage" in Path(f"/proc/{pid}/cmdline").read_bytes()}
            assert len(storage_pids) == 1
            assert any(p["id"] == "persistent" for p in api("/api/v1/collaboration/people")["people"])
            owned = children() - storage_pids
            assert len(owned) == 2, "Enabled adapters must restore their local workers"
            disabled = json.loads(json.dumps(saved))
            disabled["bindings"] = {}
            for adapter in disabled["adapters"]:
                adapter["enabled"] = False
            for service in disabled["services"]:
                if service["kind"] != "storage": service["enabled"] = False
            api("/api/v1/collaboration/services", disabled)
            wait_removed(owned)
            assert children() == storage_pids, "Disabling execution must keep durable storage alive"
            print("Packaged local runtime: four protocol bindings, reuse, restart and disable cleanup passed")
        finally:
            host.terminate()
            host.wait(timeout=12)
            wait_removed(owned)


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
