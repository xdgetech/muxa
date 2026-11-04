#!/usr/bin/env python3
"""Keep durable session identity in the same snapshot as tmux-resurrect."""
import json
import os
from pathlib import Path
import shlex
import socket
import subprocess
import sys

PREFIX = "agents-metadata\t"
OPTIONS = ("@agent_repo", "@agent_repo_dir", "@agent_branch",
           "@agent_worktree", "@agent_task", "@muxa_task")


def tmux(*args):
    command = ["tmux"]
    if os.environ.get("TMUX_AGENTS_SOCKET"):
        command += ["-L", os.environ["TMUX_AGENTS_SOCKET"]]
    return subprocess.run(command + list(args), check=True, capture_output=True,
                          text=True).stdout.removesuffix("\n")


def global_option(key):
    return tmux("show-options", "-gqv", key)


def sessions():
    for sid in tmux("list-sessions", "-F", "#{session_id}").splitlines():
        if not tmux("show-options", "-qv", "-t", sid, "@muxa_device"):
            yield sid, tmux("display-message", "-p", "-t", sid, "#{session_name}")


def save(path):
    records = []
    for sid, name in sessions():
        tags = {key: tmux("show-options", "-qv", "-t", sid, key) for key in OPTIONS}
        records.append({"name": name, "tags": tags})
    metadata = {"version": 1, "sessions": sorted(records, key=lambda row: row["name"])}
    with Path(path).open("a", encoding="utf-8") as stream:
        stream.write(PREFIX + json.dumps(metadata, sort_keys=True) + "\n")


def last_snapshot():
    configured = global_option("@resurrect-dir")
    if configured:
        directory = configured.replace("$HOME", str(Path.home()))
        directory = directory.replace("$HOSTNAME", socket.gethostname())
        return Path(os.path.expanduser(directory)) / "last"
    legacy = Path.home() / ".tmux/resurrect"
    data = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    return (legacy if legacy.is_dir() else data / "tmux/resurrect") / "last"


def restore(path):
    path = Path(path) if path else last_snapshot()
    if not path.is_file():
        return
    metadata = None
    with path.open(encoding="utf-8", errors="replace") as stream:
        for line in stream:
            if line.startswith(PREFIX):
                metadata = json.loads(line[len(PREFIX):])
    if metadata is None:
        return  # Old snapshots cannot safely identify mixed-repository sessions.
    if metadata.get("version") != 1 or not isinstance(metadata.get("sessions"), list):
        raise ValueError("unsupported metadata format")
    records = {}
    for record in metadata["sessions"]:
        name, tags = record["name"], record["tags"]
        if (not isinstance(name, str) or name in records or not isinstance(tags, dict)
                or any(key not in OPTIONS or not isinstance(value, str)
                       for key, value in tags.items())):
            raise ValueError("invalid session metadata")
        records[name] = tags
    # Names are persistent identities in resurrect; tmux targets use current ids.
    for sid, name in sessions():
        tags = records.get(name, {})
        current = {key: tmux("show-options", "-qv", "-t", sid, key) for key in tags}
        # Do not mix a saved Task identity with a different live session.
        if any(current[key] and value and current[key] != value
               for key, value in tags.items()):
            continue
        for key, value in tags.items():
            if value and not current[key]:
                tmux("set-option", "-t", sid, key, value)


def install():
    for action, event in (("save", "post-save-layout"), ("restore", "post-restore-all")):
        key = "@resurrect-hook-" + event
        installed = "@agents_resurrect_installed_" + action
        previous = "@agents_resurrect_previous_" + action
        current = global_option(key)
        if current != global_option(installed):
            tmux("set-option", "-g", previous, current)
        command = "python3 " + shlex.quote(str(Path(__file__).resolve())) + " hook " + action
        tmux("set-option", "-g", key, command)
        tmux("set-option", "-g", installed, command)


def main():
    action = sys.argv[1]
    args = sys.argv[2:]
    if action == "install":
        install()
        return
    if action == "hook":
        action, args = args[0], args[1:]
        previous = global_option("@agents_resurrect_previous_" + action)
        if previous:
            subprocess.run(["sh", "-c", previous + ' "$@"', "agents-hook"] + args,
                           check=True)
    if action == "save":
        save(args[0])
    elif action == "restore":
        restore(args[0] if args else None)
        root = Path(__file__).resolve().parent
        subprocess.run([str(root / "refresh.sh")], check=True)
    else:
        raise ValueError("expected install, save or restore")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, IndexError,
            subprocess.CalledProcessError) as error:
        # Never include tag values or saved content in error output.
        print("tmux-agents: metadata recovery failed (" + type(error).__name__ + ")",
              file=sys.stderr)
        sys.exit(1)
