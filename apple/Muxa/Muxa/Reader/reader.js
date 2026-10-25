// Renders one Markdown document. The app calls render(text, path) whenever the
// document changes; links go back to the app, which decides what opens.
"use strict";

(function () {
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: dark ? "dark" : "default" });

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
  window.render = async function (text, path, first) {
    const a = first ? { id: null, top: 0 } : anchor();
    slugs.clear();
    const html = DOMPurify.sanitize(marked.parse(text), { USE_PROFILES: { html: true } });
    const doc = document.getElementById("doc");
    doc.innerHTML = html;
    current = path;
    await drawDiagrams(doc);
    restore(a);
    post({ type: "rendered", path });
  };

  function post(message) {
    if (window.webkit && window.webkit.messageHandlers.reader) {
      window.webkit.messageHandlers.reader.postMessage(message);
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
})();
