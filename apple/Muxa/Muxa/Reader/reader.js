// Renders one Markdown document. The app calls render(text, path) whenever the
// document changes; links go back to the app, which decides what opens.
"use strict";

(function () {
  // The app's theme (ADR 0091): token key -> "#rrggbb", plus `appearance`.
  // This map is the one place that names which token colors what, for both apps.
  const cssVariables = {
    "--bg": "ground", "--fg": "text", "--muted": "textSecondary", "--faint": "textTertiary",
    "--surface": "surface", "--raised": "card", "--border": "border", "--border-strong": "borderStrong",
    "--link": "accent", "--code-fg": "terminalForeground", "--error": "diffRemoved",
    "--added": "diffAdded", "--added-text": "okText",
  };
  const muxaDark = {
    appearance: "dark", ground: "#0E1014", text: "#E7E9EC", textTertiary: "#8B94A0", card: "#161A20",
    surface: "#14171C", borderStrong: "#2C323C", readBack: "#14263F",
  };

  function initMermaid(t) {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: {
        darkMode: t.appearance !== "light",
        background: t.ground,
        primaryColor: t.card,
        primaryTextColor: t.text,
        primaryBorderColor: t.borderStrong,
        secondaryColor: t.readBack,
        tertiaryColor: t.surface,
        lineColor: t.textTertiary,
        textColor: t.text,
        fontFamily: "system-ui, -apple-system, Roboto, sans-serif",
      },
    });
  }

  /** @param {Record<string, string>} t a resolved theme */
  function applyTheme(t) {
    const root = document.documentElement.style;
    for (const [variable, token] of Object.entries(cssVariables)) {
      if (/^#[0-9a-fA-F]{6}$/.test(t[token] || "")) root.setProperty(variable, t[token]);
    }
    root.setProperty("color-scheme", t.appearance === "light" ? "light" : "dark");
    initMermaid({ ...muxaDark, ...t });
  }

  // The app may set window.muxaTheme before this file runs, so the first paint has the theme.
  applyTheme(window.muxaTheme || muxaDark);

  const escape = (s) => s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  const slugs = new Map();
  function slug(text) {
    const base = text.toLowerCase().trim().replace(/[^\p{L}\p{N}\s-]/gu, "").replace(/\s+/g, "-") || "section";
    const n = slugs.get(base) || 0;
    slugs.set(base, n + 1);
    return n ? `${base}-${n}` : base;
  }

  marked.use({
    gfm: true,
    renderer: {
      code({ text, lang }) {
        // Diagrams are drawn after sanitizing; until then they are plain text.
        if ((lang || "").trim() === "mermaid") return `<pre class="mermaid-source">${escape(text)}</pre>`;
        const cls = lang ? ` class="language-${escape(lang)}"` : "";
        return `<pre><code${cls}>${escape(text)}</code></pre>`;
      },
      heading({ tokens, depth }) {
        const inner = this.parser.parseInline(tokens);
        const plain = inner.replace(/<[^>]+>/g, "");
        return `<h${depth} id="${escape(slug(plain))}">${inner}</h${depth}>`;
      },
    },
  });

  let diagram = 0;
  async function drawDiagrams(root) {
    for (const pre of root.querySelectorAll("pre.mermaid-source")) {
      const source = pre.textContent;
      try {
        const { svg } = await mermaid.render(`mermaid-${diagram++}`, source);
        const figure = document.createElement("figure");
        figure.className = "mermaid";
        figure.innerHTML = svg;
        pre.replaceWith(figure);
      } catch (e) {
        // Shown as code, with the reason.
        pre.classList.add("mermaid-error");
        pre.title = String(e && e.message ? e.message : e);
      }
    }
  }

  // The first heading at or below the top of the view, and how far it is from it.
  function anchor() {
    const headings = document.querySelectorAll("h1, h2, h3, h4, h5, h6");
    let best = null;
    for (const h of headings) {
      const top = h.getBoundingClientRect().top;
      if (top > 0) break;
      best = { id: h.id, top };
    }
    return best ? best : { id: null, top: -window.scrollY };
  }

  function restore(a) {
    if (a.id) {
      const h = document.getElementById(a.id);
      if (h) {
        window.scrollTo(0, window.scrollY + h.getBoundingClientRect().top - a.top);
        return;
      }
    }
    window.scrollTo(0, -a.top);
  }

  let current = null;
  // What is on the page, so a new theme can draw it again.
  let shown = null;
  window.render = async function (text, path, first) {
    shown = { draw: window.render, text, path };
    const a = first ? { id: null, top: 0 } : anchor();
    slugs.clear();
    const html = DOMPurify.sanitize(marked.parse(text), { USE_PROFILES: { html: true } });
    const doc = document.getElementById("doc");
    doc.className = "";
    doc.innerHTML = html;
    current = path;
    await drawDiagrams(doc);
    restore(a);
    post({ type: "rendered", path });
  };

  // One row of the diff table. Cells are text only: nothing in a diff becomes HTML.
  function row(kind, oldNo, newNo, text) {
    const tr = document.createElement("tr");
    tr.className = kind;
    for (const [cls, value] of [["num", oldNo], ["num", newNo], ["code", text]]) {
      const td = document.createElement("td");
      td.className = cls;
      td.textContent = value === null ? "" : String(value);
      tr.appendChild(td);
    }
    return tr;
  }

  // A unified diff as GitHub shows it: both line numbers, then the line with its sign.
  window.renderDiff = function (text, path, first) {
    shown = { draw: window.renderDiff, text, path };
    const y = first ? 0 : window.scrollY;
    const table = document.createElement("table");
    table.className = "diff";
    let oldNo = 0;
    let newNo = 0;
    let inHunk = false;
    for (const line of text.split("\n")) {
      const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
      if (hunk) {
        oldNo = Number(hunk[1]);
        newNo = Number(hunk[2]);
        inHunk = true;
        table.appendChild(row("hunk", null, null, line));
      } else if (!inHunk) {
        // The header before the first hunk: names and modes, already in the title.
        continue;
      } else if (line.startsWith("+")) {
        table.appendChild(row("add", null, newNo++, line));
      } else if (line.startsWith("-")) {
        table.appendChild(row("del", oldNo++, null, line));
      } else if (line.startsWith(" ")) {
        table.appendChild(row("ctx", oldNo++, newNo++, line));
      } else if (line.startsWith("\\")) {
        table.appendChild(row("note", null, null, line.slice(2)));
      } else if (line.startsWith("diff --git")) {
        inHunk = false;
      }
    }
    const scroll = document.createElement("div");
    scroll.className = "diff-scroll";
    scroll.appendChild(table);
    const doc = document.getElementById("doc");
    doc.className = "diff";
    doc.replaceChildren(scroll);
    current = path;
    window.scrollTo(0, y);
    post({ type: "rendered", path });
  };

  function post(message) {
    if (window.webkit && window.webkit.messageHandlers.reader) {
      window.webkit.messageHandlers.reader.postMessage(message);
    } else if (window.muxaReader) {
      // Android: a JavascriptInterface takes strings only.
      window.muxaReader.postMessage(JSON.stringify(message));
    }
  }

  document.addEventListener("click", (event) => {
    const link = event.target.closest("a[href]");
    if (!link) return;
    event.preventDefault();
    const href = link.getAttribute("href");
    if (href.startsWith("#")) {
      const target = document.getElementById(decodeURIComponent(href.slice(1)));
      if (target) target.scrollIntoView();
      return;
    }
    post({ type: "link", href, from: current });
  });

  // A new theme from the app: the colors change at once, and diagrams are drawn again in them.
  window.setTheme = function (theme) {
    window.muxaTheme = theme;
    applyTheme(theme);
    if (shown) shown.draw(shown.text, shown.path, false);
  };
})();
