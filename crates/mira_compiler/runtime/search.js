// <mira-search>: client search over /_mira/search.json. Loaded only on
// pages that use the element. Press / anywhere to focus it.
const MAX = 8;
let index;

async function load() {
  index ??= fetch("/_mira/search.json").then((r) => r.json()).then((docs) =>
    docs.map((d) => ({
      ...d,
      t: d.title.toLowerCase(),
      h: d.headings.map((h) => h.text.toLowerCase()),
      d: (d.description || "").toLowerCase(),
      x: d.text.toLowerCase(),
    })),
  );
  return index;
}

function score(doc, terms) {
  let total = 0;
  let anchor = null;
  for (const term of terms) {
    let s = 0;
    if (doc.t.includes(term)) s += doc.t.startsWith(term) ? 14 : 10;
    const hi = doc.h.findIndex((h) => h.includes(term));
    if (hi >= 0) {
      s += 6;
      anchor ??= doc.headings[hi];
    }
    if (doc.d.includes(term)) s += 3;
    if (doc.x.includes(term)) s += 1;
    if (!s) return null;
    total += s;
  }
  return { doc, total, anchor };
}

function snippet(doc, terms) {
  const at = Math.max(0, doc.x.indexOf(terms[0]));
  const start = Math.max(0, at - 50);
  const text = doc.text.slice(start, start + 150);
  return (start > 0 ? "…" : "") + text + (start + 150 < doc.text.length ? "…" : "");
}

function highlight(node, text, terms) {
  const lower = text.toLowerCase();
  let i = 0;
  while (i < text.length) {
    let next = -1, len = 0;
    for (const t of terms) {
      const at = lower.indexOf(t, i);
      if (at >= 0 && (next < 0 || at < next)) { next = at; len = t.length; }
    }
    if (next < 0) { node.append(text.slice(i)); break; }
    node.append(text.slice(i, next));
    const mark = document.createElement("mark");
    mark.textContent = text.slice(next, next + len);
    node.append(mark);
    i = next + len;
  }
}

class MiraSearch extends HTMLElement {
  connectedCallback() {
    if (this.input) return;
    const id = `mira-search-${Math.random().toString(36).slice(2, 8)}`;
    this.input = Object.assign(document.createElement("input"), {
      type: "search",
      placeholder: this.getAttribute("placeholder") || "Search",
      autocomplete: "off",
      spellcheck: false,
    });
    this.input.setAttribute("aria-label", "Search the site");
    this.input.setAttribute("aria-controls", id);
    const key = document.createElement("kbd");
    key.textContent = "/";
    this.list = document.createElement("ol");
    this.list.id = id;
    this.list.setAttribute("role", "list");
    this.list.hidden = true;
    this.append(this.input, key, this.list);

    this.input.addEventListener("focus", load, { once: true });
    this.input.addEventListener("input", () => this.run());
    this.addEventListener("keydown", (e) => this.keys(e));
    document.addEventListener("keydown", (e) => {
      if (e.key === "/" && !/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName) && !document.activeElement?.isContentEditable) {
        e.preventDefault();
        this.input.focus();
      }
    });
    document.addEventListener("click", (e) => { if (!this.contains(e.target)) this.list.hidden = true; });
  }

  async run() {
    const q = this.input.value.trim().toLowerCase();
    const terms = q.split(/\s+/).filter(Boolean);
    this.list.replaceChildren();
    if (!terms.length) { this.list.hidden = true; return; }
    const docs = await load();
    if (this.input.value.trim().toLowerCase() !== q) return;
    const hits = docs.map((d) => score(d, terms)).filter(Boolean).sort((a, b) => b.total - a.total).slice(0, MAX);
    if (!hits.length) {
      const li = document.createElement("li");
      li.className = "empty";
      li.textContent = `No results for “${this.input.value.trim()}”`;
      this.list.append(li);
    }
    for (const { doc, anchor } of hits) {
      const li = document.createElement("li");
      const a = document.createElement("a");
      a.href = anchor ? `${doc.url}#${anchor.id}` : doc.url;
      const title = document.createElement("strong");
      highlight(title, anchor ? `${doc.title} · ${anchor.text}` : doc.title, terms);
      const text = document.createElement("span");
      highlight(text, snippet(doc, terms), terms);
      a.append(title, text);
      li.append(a);
      this.list.append(li);
    }
    this.list.hidden = false;
  }

  keys(e) {
    const links = [...this.list.querySelectorAll("a")];
    const at = links.indexOf(document.activeElement);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      links[Math.min(at + 1, links.length - 1)]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      (at <= 0 ? this.input : links[at - 1]).focus();
    } else if (e.key === "Escape") {
      this.list.hidden = true;
      this.input.focus();
    }
  }
}

customElements.define("mira-search", MiraSearch);
