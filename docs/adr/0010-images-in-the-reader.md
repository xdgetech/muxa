# 0010: Images in the Reader, ignored folders included

## Status

Accepted.

## Context

Automatic QA runs write screenshots, and checking them is part of reviewing an agent's work. The Reader showed Markdown, plans and diffs only. Screenshots usually sit in folders git ignores, such as `build/` or `test-results/`. The Reader lists only files that git tracks or does not ignore, so that an ignored secret is never served (ADR 0006).

## Decision

- **New requests `images` and `image`, behind the capability `images`.** They mirror `docs` and `doc`: a list, then one open image that is sent again when its time or size changes (checked every 2 s). A new doc kind was rejected: the Swift `DocInfo.Kind` is a strict enum, so older apps would fail to decode the whole list.
- **Ignored files are listed, but only by image extension.** `git ls-files --cached --others` without `--exclude-standard`, limited to PNG, JPEG, GIF, WebP and HEIC in any case. An ignored `.env` or key file is never listed, because its extension is not an image's. SVG is left out: it can carry scripts.
- **Newest first, at most 2000.** The newest QA run survives the cap. The app shows the list as a folder tree, with the newest image's folders open.
- **Reading uses the Reader's checks:** the path must be listed, the real path must be inside the repo, and the file must be at most 16 MiB. Its base64 line then stays under the app's 32 MiB line limit.

## Consequences

- An image in an ignored folder can be private, for example a screenshot of a login page. Only someone who can already SSH to the host as this user can see it, and the Reader already shows the repo.
- Listing walks ignored folders. On this repo, with 68,000 ignored files in `target/`, it takes about 200 ms. It runs when the Reader opens, not on a timer.
- A rewritten image is sent whole. A 2752×2064 screenshot is a few MB, sent only when the file changed.
