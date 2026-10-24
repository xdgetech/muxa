"""Claude Code PreToolUse hook: blocks Bash commands that run tmux against the
default server, where the developer's live sessions run. tmux with -L or -S
(an isolated server) and `tmux -V` pass.

Input: the hook JSON on stdin. Exit 2 blocks the call and shows stderr to Claude.
"""
import json
import os
import re
import shlex
import sys

BLOCK_MESSAGE = (
    "muxa guard: this tmux command targets the default tmux server, where the "
    "developer's live sessions run. Use an isolated server instead: "
    "`tmux -L muxa-test-<name> ...` or `scripts/tmux-sandbox.sh up`. If the user "
    "asked for a read of their live server, ask them to run it with `!`."
)
SEPARATORS = {";", "&", "&&", "|", "||", "(", ")", "$(", "|&"}
WRAPPERS = {"env", "command", "exec", "sudo", "nohup", "time", "nice"}
SHELLS = {"sh", "bash", "zsh", "dash"}
ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
# A flag cluster that contains -L or -S, e.g. -L, -Lname, -uL.
SERVER_FLAG = re.compile(r"^-[A-Za-z]*[LS]")


def segments(command):
    """Splits a shell command into simple commands, as lists of words."""
    text = command.replace("`", ";").replace("\n", ";")
    lexer = shlex.shlex(text, posix=True, punctuation_chars=";&|()")
    lexer.whitespace_split = True
    current = []
    for token in lexer:
        if token in SEPARATORS or set(token) <= set(";&|()"):
            if current:
                yield current
            current = []
        else:
            current.append(token)
    if current:
        yield current


def is_tmux(word):
    return os.path.basename(word) == "tmux"


def blocked_tmux(args):
    """args: the words after `tmux`. True when they reach the default server."""
    skip = False
    for arg in args:
        if skip:
            skip = False
            continue
        if not arg.startswith("-"):
            return True
        if arg == "-V" or SERVER_FLAG.match(arg):
            return False
        # -c, -f and -T take a value; a cluster such as -uf ends with the flag.
        skip = arg[-1] in "cfT"
    return True


def blocked(command):
    for words in segments(command):
        while words and (ASSIGNMENT.match(words[0]) or words[0] in WRAPPERS):
            words = words[1:]
        if not words:
            continue
        if is_tmux(words[0]) and blocked_tmux(words[1:]):
            return True
        if os.path.basename(words[0]) in SHELLS:
            # sh -c 'tmux ...', also as a cluster such as -lc.
            flags = [i for i, w in enumerate(words) if re.match(r"^-[A-Za-z]*c$", w)]
            if flags and flags[0] + 1 < len(words) and blocked(words[flags[0] + 1]):
                return True
    return False


def blocked_fallback(command):
    """For commands shlex cannot parse: any tmux word without -L/-S anywhere."""
    if not re.search(r"(^|[\s;&|`(])tmux(\s|$)", command):
        return False
    return not re.search(r"\s-[A-Za-z]*[LS]", command) and " -V" not in command


def main():
    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0
    command = (event.get("tool_input") or {}).get("command") or ""
    if "tmux" not in command:
        return 0
    try:
        hit = blocked(command)
    except ValueError:
        hit = blocked_fallback(command)
    if hit:
        print(BLOCK_MESSAGE, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
