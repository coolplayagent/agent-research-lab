"""Exercise direct local knowledge setup, separate ownership and restart with real packaged processes."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request
import urllib.error


def run(archive):
    with tempfile.TemporaryDirectory(prefix="oh-knowledge-", dir="/tmp") as temp:
        root = Path(temp)
        with tarfile.open(archive) as package:
            package.extractall(root, filter="data")
        binaries = root / "outcome-hub"
        knowledge = binaries / "outcome-hub-knowledge"
        assert knowledge.is_file()
        state = root / "state"
        directory = root / "notes"
        directory.mkdir()
        (directory / "note.md").write_text("original source\n")
        repository = root / "repository"
        repository.mkdir()
        (repository / "note.md").write_text("git source\n")
        for args in [("init", "-b", "main"), ("add", "."), ("-c", "user.name=Fixture", "-c", "user.email=test@example.org", "-c", "commit.gpgSign=false", "commit", "-m", "baseline"), ("branch", "publish")]:
            subprocess.run(["git", "-C", str(repository), *args], check=True, capture_output=True)
        with socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        def start():
            with (root / "host.log").open("ab") as log:
                return subprocess.Popen([str(binaries / "outcome-hub"), "serve", "--state-dir", str(state), "--listen", f"127.0.0.1:{port}", "--max-seconds", "180"], stdout=log, stderr=log)
        host = start()
        def api(path, body=None, intent="manage-knowledge", origin=None):
            request = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers={"Content-Type":"application/json", "Origin":origin or base, "X-Crystal-Intent":intent})
            with urllib.request.urlopen(request, timeout=30) as response:
                return json.load(response)
        def ready():
            for _ in range(100):
                assert host.poll() is None, (root / "host.log").read_text()
                try:
                    api("/api/operator/session")
                    return
                except OSError:
                    time.sleep(.1)
            raise AssertionError("Host did not become ready")
        def config():
            return api("/api/v1/collaboration/services")["configuration"]
        def local(operation, target, name="notes"):
            request = {"operation":operation, "target":target}
            if operation == "add":
                request["name"] = name
            return api("/api/v1/knowledge/local", {"configuration_revision":config()["revision"], "request":request})
        def reject(call):
            try:
                call()
            except urllib.error.HTTPError as error:
                assert error.code in (400, 403, 409, 422, 500, 503), error.code
                return
            raise AssertionError("Invalid knowledge setup was accepted")
        def children():
            result = subprocess.run(["ps", "-o", "pid=", "--ppid", str(host.pid)], capture_output=True, text=True)
            return {int(value) for value in result.stdout.split()}
        def stopped(pids):
            for _ in range(100):
                if all(not Path(f"/proc/{pid}").exists() for pid in pids):
                    return
                time.sleep(.1)
            raise AssertionError("Owned knowledge process survived host shutdown")
        target = {"kind":"directory", "path":str(directory), "content_root":".", "enabled":True, "access":"read_only", "reference":None, "remote":None, "credential_env":[]}
        try:
            ready()
            assert config()["outcome_services"] == []
            assert not children(), "Knowledge must not start until requested"
            hidden = knowledge.with_suffix(".disabled")
            knowledge.rename(hidden)
            try:
                reject(lambda: local("inspect", target))
                assert config()["outcome_services"] == [], "Missing runtime must not create a configured target"
            finally:
                hidden.rename(knowledge)
            reject(lambda: api("/api/v1/knowledge/local", {"configuration_revision":0, "request":{"operation":"inspect", "target":target}}, origin="https://foreign.invalid"))
            inspected = local("inspect", target)
            assert inspected["result"]["target"] == target
            service = inspected["service_id"]
            assert len(children()) == 1
            added = local("add", target)
            assert added["result"]["configuration"]["revision"] == 1
            assert local("add", target)["result"]["configuration"]["revision"] == 1
            invalid = {**target, "path":str(root / "missing")}
            reject(lambda: local("add", invalid, "missing"))
            reject(lambda: local("add", {**target, "path":"relative"}, "invalid"))
            reject(lambda: local("add", {**target, "path":str(state)}, "internal"))
            reject(lambda: local("add", {**target, "access":"read_write"}))
            git = {**target, "kind":"git", "path":str(repository)}
            detected = local("inspect", git)
            assert detected["result"]["target"]["reference"] == "refs/heads/main"
            local("add", git, "repo")
            reject(lambda: local("add", {**git, "access":"read_write"}, "unsafe"))
            reject(lambda: local("add", {**git, "access":"read_write", "reference":"refs/heads/main"}, "unsafe"))
            local("add", {**git, "access":"read_write", "reference":"refs/heads/publish"}, "publish")
            assert len(config()["outcome_services"]) == 1
            assert config().get("knowledge_groups", {}) == {} and config().get("knowledge_goals", {}) == {}
            def proxy(request):
                return api("/api/v1/knowledge", {"service_id":service, "configuration_revision":config()["revision"], "request":request})
            assert proxy({"operation":"read", "target":"notes", "path":"note.md", "revision":None})["content"] == "original source\n"
            assert proxy({"operation":"read", "target":"repo", "path":"note.md", "revision":None})["content"] == "git source\n"
            reject(lambda: proxy({"operation":"read", "target":"notes", "path":"../outside", "revision":None}))
            saved = proxy({"operation":"list"})
            assert set(saved["targets"]) == {"notes", "repo", "publish"}
            assert subprocess.check_output(["git", "-C", str(repository), "status", "--porcelain"]) == b""
            original = config()
            owned = children()
            host.terminate()
            host.wait(timeout=15)
            stopped(owned)
            host = start()
            ready()
            assert config() == original
            assert proxy({"operation":"list"}) == saved
            assert len(children()) == 1
            changed = config()
            changed["outcome_services"][0]["enabled"] = False
            api("/api/v1/collaboration/services", changed, "manage-services")
            stopped(children())
            reject(lambda: local("add", target, "disabled"))
        finally:
            if host.poll() is None:
                owned = children()
                host.terminate()
                host.wait(timeout=15)
                stopped(owned)
        print("Direct Git/directory add, revisions, read-only defaults, isolated ownership and restart passed")


if __name__ == "__main__":
    run(sys.argv[1])
