#!/usr/bin/env python3
"""Run QA on existing, host-configured simulators under one user-wide lock."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import tempfile


ROLES = {
    "iphone": "com.apple.CoreSimulator.SimDeviceType.iPhone-18-Pro-Max",
    "ipad-mini": "com.apple.CoreSimulator.SimDeviceType.iPad-mini-A17-Pro",
    "ipad-pro": "com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M5-12GB",
}
LOCK_PATH = "/tmp/ios-qa-{}.lock".format(os.getuid())


def config_path():
    override = os.environ.get("IOS_QA_CONFIG")
    if override:
        return Path(override).expanduser()
    host = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "ios-qa/config.json"
    if host.is_file():
        return host
    # All worktrees of this checkout use the same ignored host configuration.
    repo = Path(__file__).resolve().parent.parent
    common = subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "--git-common-dir"], text=True
    ).strip()
    return (repo / common).resolve().parent / ".ios-qa.local.json"


def storage(config):
    volume = Path(config["volume"]).expanduser().resolve()
    root = Path(config["storage_root"]).expanduser().resolve()
    if not volume.is_mount():
        raise ValueError("configured QA volume is not mounted; no fallback storage is used")
    if root == volume or volume not in root.parents:
        raise ValueError("storage_root must be inside the mounted QA volume")
    # Check the closest existing ancestor before creating any output directories.
    ancestor = root
    while not ancestor.exists():
        ancestor = ancestor.parent
    if ancestor.stat().st_dev != volume.stat().st_dev:
        raise ValueError("QA storage is on a different filesystem")
    return root


def storage_directory(root, relative):
    path = (root / relative).resolve()
    if root not in path.parents:
        raise ValueError("QA output directory escapes configured storage")
    ancestor = path
    while not ancestor.exists():
        ancestor = ancestor.parent
    if ancestor.stat().st_dev != root.stat().st_dev:
        raise ValueError("QA output directory is on a different filesystem")
    path.mkdir(parents=True, exist_ok=True)
    return path


def destinations(config, roles):
    devices = json.loads(subprocess.check_output(
        ["xcrun", "simctl", "list", "devices", "--json"], text=True
    ))["devices"]
    selected = {}
    for role in roles:
        entry = config["devices"][role]
        selector = entry["udid"]
        # Retain the existing Makefile overrides, validating their actual type.
        override = {"ipad-mini": "MUXA_SIM", "iphone": "MUXA_SIM_IPHONE"}.get(role)
        if override and os.environ.get(override):
            selector = os.environ[override]
        matches = [d for runtime, ds in devices.items() for d in ds
                   if runtime == entry["runtime"] and d.get("isAvailable") is True
                   and selector in (d["udid"], d["name"])]
        if len(matches) != 1:
            raise ValueError(role + ": expected exactly one available configured device; none will be created")
        device = matches[0]
        if device.get("deviceTypeIdentifier") != ROLES[role]:
            raise ValueError(role + ": configured device has the wrong device type")
        selected[role] = device["udid"]
    if len(set(selected.values())) != len(selected):
        raise ValueError("QA roles must refer to distinct devices")
    return selected


def run_child(command, env, lock_fd):
    child = subprocess.Popen(command, env=env, start_new_session=True, pass_fds=(lock_fd,))
    interrupted = 0

    def stop(signum, _frame):
        nonlocal interrupted
        interrupted = signum
        # Keep the lock until the command and its cleanup traps have finished.
        try:
            os.killpg(child.pid, signum)
        except ProcessLookupError:
            pass

    previous = {sig: signal.signal(sig, stop) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        code = child.wait()
        if interrupted:
            return 128 + interrupted
        return code if code >= 0 else 128 - code
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["check", "run"])
    parser.add_argument("roles", nargs="+", choices=list(ROLES))
    # Split explicitly so command flags are never parsed as wrapper options.
    argsv = sys.argv[1:]
    split = argsv.index("--") if "--" in argsv else len(argsv)
    args = parser.parse_args(argsv[:split])
    command = argsv[split + 1:]
    if (args.action == "run") != bool(command):
        parser.error("run requires '-- command ...'; check takes no command")
    with config_path().open() as stream:
        config = json.load(stream)
    root = storage(config)
    # Never unlink lock files: that would allow two processes to lock different inodes.
    fd = os.open(LOCK_PATH, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w") as lock:
        info = os.fstat(lock.fileno())
        if info.st_uid != os.getuid() or not stat.S_ISREG(info.st_mode):
            raise ValueError("unsafe QA lock file")
        print("Waiting for the shared iOS QA lock...", file=sys.stderr)
        fcntl.flock(lock, fcntl.LOCK_EX)
        root = storage(config)
        selected = destinations(config, args.roles)
        if args.action == "check":
            print("QA volume and configured devices are available: " + ", ".join(selected))
            return 0
        project = hashlib.sha256(str(Path.cwd().resolve()).encode()).hexdigest()[:16]
        root.mkdir(parents=True, exist_ok=True)
        derived = storage_directory(root, "derived-data/" + project)
        runs = storage_directory(root, "runs/" + project)
        output = tempfile.mkdtemp(prefix="qa-", dir=str(runs))
        env = os.environ.copy()
        for role, udid in selected.items():
            env["IOS_QA_" + role.upper().replace("-", "_") + "_UDID"] = udid
        if len(selected) == 1:
            env["IOS_QA_UDID"] = next(iter(selected.values()))
        env["IOS_QA_DERIVED_DATA"] = str(derived)
        env["IOS_QA_OUTPUT_ROOT"] = output
        print("QA output: " + output, file=sys.stderr)
        return run_child(command, env, lock.fileno())


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print("iOS QA: " + str(error), file=sys.stderr)
        sys.exit(1)
