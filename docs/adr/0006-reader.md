# 0006: A Markdown Reader with a bridge-made allow list and an offline web view

## Status

Accepted.

## Context

Plans and designs are written in Markdown, often with mermaid diagrams, and an agent keeps editing them while it works. On the iPad they were only readable as raw text in a terminal. The files live on the host; the iPad reaches the host only through the bridge's SSH channel, and the channel's key can already type into panes, but should not become a way to read any file.

## Decision

- **The bridge decides what is readable.** For a pane it lists the agent's documents (adapters return them; Claude Code's plan is `plans/<slug>.md`, named by its transcript) and the Markdown files of the pane's git repo that git tracks or does not ignore. A `doc` request must name a path from that list, built again for the request. Every read checks that the real file, after symlinks, is still inside its directory, and is at most 2 MiB.
- **The bridge follows the open document** and sends the whole text again when it changes. Documents are small; sending them whole keeps the client simple and needs no diffing.
- **The app renders in a WKWebView** with marked, mermaid and DOMPurify bundled in the app, at pinned versions with recorded checksums. A content security policy allows no network: everything is local at run time. Links to other listed Markdown files open in the Reader; other links are not followed in the view.

## Consequences

- A document outside a git repo, or ignored by git, is not readable in the Reader. That is the point: secrets usually live in ignored files.
- Fetching the libraries needs the network once, when they are vendored or updated (`scripts/vendor-reader.sh`), never at build or run time.
- Mermaid adds a few megabytes to the app. A diagram mermaid cannot draw is shown as code.
- Rejected: rendering Markdown natively with mermaid in embedded web views. Sizing many web views inside a native layout is fragile, and it would mean two renderers.
