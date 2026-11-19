# 0065: HTML documents in the Reader, scripts in a sandbox

## Status

Accepted.

## Context

Agents often write a report as one `.html` file, with inline scripts for charts or tabs. The Reader showed Markdown only. Its page runs no script but its own bundled files, and DOMPurify strips the rest (ADR 0006). Opening such a report on a phone or tablet meant a desktop browser or a local server.

## Decision

- **The client asks for HTML.** `docs` and `doc` take an optional `html: true`, behind the capability `html-docs`. With it, the bridge also lists `*.html` and `*.htm`, by the same rules as Markdown: tracked or not ignored in a repo, the folder rules outside one, 2 MiB, the path checks on every read. The title is the file's `<title>`. Without it, nothing changes: an older app would render HTML as Markdown. A new doc kind was rejected for the reason in ADR 0010. The client tells HTML apart by the path.
- **The document runs in a sandbox.** The app loads a bundled host page, `html.html`, in a web view of its own. It puts the text in an `<iframe sandbox="allow-scripts" srcdoc>`. That web view has no message handler (iOS) and no JavaScript interface (Android). Without `allow-same-origin`, the document gets an opaque origin: no access to the host page, storage or cookies. No popups, forms or top navigation, and every navigation but the frame's own document is refused.
- **No network.** The frame inherits the host page's CSP: inline and `data:` scripts, styles, images and fonts only, no `connect-src`. Behind it, Android sets `blockNetworkLoads` and iOS adds a content rule list that blocks `http`, `https`, `ws`, `wss` and `ftp`.
- **One file.** Files beside the HTML (`./app.js`, `./chart.png`) are not served. A report must inline them.

## Consequences

- A report that loads Chart.js or a font from a CDN shows without them. The agent has to inline them; most libraries fit in 2 MiB.
- Agent-written JavaScript now runs on the device. It can use CPU and memory in the web view, and nothing else: no app bridge, no network, no storage. A web view that crashes takes only the page down.
- An update replaces the frame's document: scripts start again and the scroll position goes back to the top.
- Links inside a report go nowhere, except anchors within it. Inbox items do not attach HTML yet: `is_doc` for evidence stays Markdown-only.
