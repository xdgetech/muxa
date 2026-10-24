#!/bin/sh
# A stand-in for an AI agent in sandbox panes: echoes each line in color.
# "burst" prints fast output for 3 s; "alt" shows a full-screen view until Enter.
printf '\033[1;35mfake agent\033[0m ready. Try: burst, alt, or any text.\n'
while printf '\033[36m> \033[0m' && IFS= read -r line; do
  case "$line" in
    burst)
      end=$(($(date +%s) + 3)); n=0
      while [ "$(date +%s)" -lt "$end" ]; do
        n=$((n + 1)); printf '\033[3%dmline %d ünïcödé ✓\033[0m\n' $((n % 7 + 1)) "$n"
      done ;;
    alt)
      printf '\033[?1049h\033[2J\033[5;10H\033[7m full-screen view \033[0m\033[7;10HPress Enter.'
      read -r _; printf '\033[?1049l' ;;
    *) printf 'you said: \033[33m%s\033[0m\n' "$line" ;;
  esac
done
