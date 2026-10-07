// Mira client runtime. Inlined as a classic script in <head> so the
// pagereveal listener is registered before the first render of a page.
// Lines are trimmed and comment lines dropped at build time, so every
// statement ends with a semicolon.
(() => {
const d = document, nav = window.navigation;
const cfg = JSON.parse(d.getElementById("mira-config")?.textContent || "{}");
const match = (p, path) => p.endsWith("*") ? path.startsWith(p.slice(0, -1)) && path != p.slice(0, -1) : path == p;
// Frames hold their mosaic until the image loads, then resolve into it.
// Without this script images simply show.
d.documentElement.classList.add("mira-js");
const loaded = (img) => img.closest(".mira-frame")?.setAttribute("data-loaded", "");
d.addEventListener("load", (e) => { if (e.target.tagName == "IMG") loaded(e.target); }, true);
d.addEventListener("DOMContentLoaded", () => { for (const i of d.querySelectorAll(".mira-frame img")) if (i.complete) loaded(i); });
// Agents and automation get a deterministic DOM with no animation.
const agent = navigator.webdriver || /bot|crawl|spider|headless/i.test(navigator.userAgent);

addEventListener("pagereveal", (e) => {
const vt = e.viewTransition, a = nav?.activation;
if (!vt) return;
if (agent || !a?.from) return vt.skipTransition();
const from = new URL(a.from.url).pathname, to = location.pathname;
let back = a.navigationType == "traverse" && a.entry.index < a.from.index, name;
for (const [x, y, n] of cfg.pairs || []) {
if (match(x, from) && match(y, to)) { name = n; break; }
if (match(y, from) && match(x, to)) { name = n; back = true; break; }
}
name ||= d.querySelector("meta[name=mira-transition]")?.content || cfg.default || "fade";
vt.types.add(name);
vt.types.add(back ? "back" : "forward");
});

// Move focus to the main heading after a link navigation so keyboard and
// screen reader users start at the new content.
d.addEventListener("DOMContentLoaded", () => {
const t = nav?.activation?.navigationType;
if (!nav?.activation?.from || (t != "push" && t != "replace") || location.hash) return;
const h = d.querySelector("main h1") || d.querySelector("h1");
if (!h) return;
h.tabIndex = -1;
h.focus({ preventScroll: true });
});

// Browsers without Speculation Rules prefetch same origin links on hover,
// touch, or focus. Authenticated or opted out links carry data-mira-no-prefetch.
if (HTMLScriptElement.supports?.("speculationrules")) return;
const seen = new Set();
const prefetch = (e) => {
const l = e.target.closest?.("a[href]");
if (!l || l.origin != location.origin || l.pathname == location.pathname || l.hasAttribute("download") || l.closest("[data-mira-no-prefetch]") || seen.has(l.href) || navigator.connection?.saveData) return;
seen.add(l.href);
const k = d.createElement("link");
k.rel = "prefetch";
k.href = l.href;
d.head.append(k);
};
for (const t of ["pointerover", "touchstart", "focusin"]) d.addEventListener(t, prefetch, { passive: true, capture: true });
})();
