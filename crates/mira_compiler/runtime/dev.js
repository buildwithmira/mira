// Mira dev client. Long polls the dev server, reloads after a successful
// rebuild, and shows build and runtime errors in an overlay.
const build = document.querySelector("meta[name=mira-build]")?.content ?? "";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const state = { build: null, runtime: [], open: true, index: 0 };
let version = "";

async function poll() {
  for (;;) {
    try {
      const res = await fetch(`/_mira/wait?v=${version}`, { cache: "no-store" });
      const status = await res.json();
      version = status.version;
      if (status.error) {
        state.build = status.error;
        state.open = true;
        render();
      } else if (status.build_id !== build) {
        return location.reload();
      } else if (state.build) {
        state.build = null;
        render();
      }
    } catch {
      await sleep(1000);
    }
  }
}

addEventListener("error", (e) => {
  if (!e.error && !e.message) return;
  pushRuntime({ message: e.message || String(e.error), file: e.filename, line: e.lineno, stack: e.error?.stack });
});
addEventListener("unhandledrejection", (e) => {
  const r = e.reason;
  pushRuntime({ message: `Unhandled rejection: ${r?.message ?? r}`, stack: r?.stack });
});

function pushRuntime(error) {
  state.runtime.push(error);
  state.open = true;
  state.index = state.runtime.length - 1;
  render();
}

let host, root;
function mount() {
  if (host) return;
  host = document.createElement("mira-dev-overlay");
  root = host.attachShadow({ mode: "open" });
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = "/_mira/overlay.css";
  root.append(css);
  document.documentElement.append(host);
  addEventListener("keydown", (e) => {
    if (e.key === "Escape" && state.open) {
      state.open = false;
      render();
    }
  });
}

const el = (tag, cls, text) => {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text != null) node.textContent = text;
  return node;
};

// The Mira mark: the 5 by 5 pixel M, unlit pixels dithered.
const MARK = ["#...#", "##.##", "#.#.#", "#...#", "#...#"];
function mark() {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 4.875 4.875");
  svg.setAttribute("class", "mark");
  svg.setAttribute("aria-hidden", "true");
  MARK.forEach((row, y) => [...row].forEach((c, x) => {
    const px = document.createElementNS(ns, "rect");
    for (const [k, v] of [["x", x], ["y", y], ["width", 0.875], ["height", 0.875], ["rx", 0.11]]) px.setAttribute(k, v);
    if (c !== "#") px.setAttribute("class", "off");
    svg.append(px);
  }));
  return svg;
}

// Plain text version of an error, for pasting into an issue or a chat.
function report(error) {
  const lines = [`${error.kind}: ${error.message}`];
  if (error.file) lines.push(`at ${error.line ? `${error.file}:${error.line}` : error.file}`);
  if (error.excerpt?.length) {
    lines.push("");
    for (const l of error.excerpt) lines.push(`${l.current ? ">" : " "} ${String(l.number).padStart(4)} | ${l.text}`);
  } else if (error.stack) {
    lines.push("", error.stack);
  }
  if (error.hint) lines.push("", `hint: ${error.hint}`);
  for (const cause of error.causes || []) lines.push(`cause: ${cause}`);
  return lines.join("\n");
}

function render() {
  mount();
  for (const node of [...root.children]) if (node.tagName !== "LINK") node.remove();
  const errors = [...(state.build ? [{ kind: "Build error", ...state.build }] : []), ...state.runtime.map((e) => ({ kind: "Runtime error", ...e }))];
  if (!errors.length) return;
  state.index = Math.min(state.index, errors.length - 1);

  if (!state.open) {
    const pill = el("button", "pill");
    pill.append(mark(), el("span", null, `${errors.length} ${errors.length === 1 ? "error" : "errors"}`));
    pill.onclick = () => { state.open = true; render(); };
    root.append(pill);
    return;
  }

  const error = errors[state.index];
  const backdrop = el("div", "backdrop");
  const panel = el("section", "panel");
  panel.setAttribute("role", "alertdialog");
  panel.setAttribute("aria-modal", "true");
  panel.setAttribute("aria-labelledby", "mira-error-title");

  const head = el("header", "head");
  head.append(mark(), el("span", "kind", error.kind));
  if (errors.length > 1) {
    const nav = el("span", "nav");
    const prev = el("button", "step", "‹");
    const next = el("button", "step", "›");
    prev.setAttribute("aria-label", "Previous error");
    next.setAttribute("aria-label", "Next error");
    prev.onclick = () => { state.index = (state.index - 1 + errors.length) % errors.length; render(); };
    next.onclick = () => { state.index = (state.index + 1) % errors.length; render(); };
    nav.append(prev, el("span", "count", `${state.index + 1} of ${errors.length}`), next);
    head.append(nav);
  }
  const copy = el("button", "copy", "Copy");
  copy.setAttribute("aria-label", "Copy error details");
  copy.onclick = async () => {
    try {
      await navigator.clipboard.writeText(report(error));
      copy.textContent = "Copied";
    } catch {
      copy.textContent = "Copy failed";
    }
    setTimeout(() => (copy.textContent = "Copy"), 1600);
  };
  head.append(copy);

  const close = el("button", "close", "Esc");
  close.setAttribute("aria-label", "Dismiss");
  close.onclick = () => { state.open = false; render(); };
  head.append(close);

  const title = el("h1", "message", error.message);
  title.id = "mira-error-title";
  panel.append(head, title);

  const where = error.file ? (error.line ? `${error.file}:${error.line}` : error.file) : null;
  if (where) panel.append(el("p", "loc", where));

  if (error.excerpt?.length) {
    const code = el("pre", "code");
    const width = String(error.excerpt.at(-1).number).length;
    for (const line of error.excerpt) {
      const row = el("span", line.current ? "row current" : "row");
      row.append(el("span", "ln", String(line.number).padStart(width)), el("span", "src", line.text || " "));
      code.append(row);
    }
    panel.append(code);
  } else if (error.stack) {
    panel.append(el("pre", "code stack", error.stack));
  }

  if (error.hint) {
    const hint = el("p", "hint");
    hint.append(el("span", "label", "Hint"), el("span", null, error.hint));
    panel.append(hint);
  }
  if (error.causes?.length) {
    const list = el("ul", "causes");
    for (const cause of error.causes) list.append(el("li", null, cause));
    panel.append(list);
  }
  panel.append(el("footer", "foot", error.kind === "Build error"
    ? "Save a fix and the page reloads once the build succeeds."
    : "This error was thrown in the browser. Reload after fixing it."));

  backdrop.append(panel);
  root.append(backdrop);
  close.focus({ preventScroll: true });
}

poll();
