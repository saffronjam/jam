"use strict";

const markdown = window.markdownit({ html: false });
// Images alongside the document come from its pane's machine. External images
// stay links: rendering a README should never contact an unrelated service.
markdown.renderer.rules.image = (tokens, index) => {
  const token = tokens[index];
  const src = token.attrGet("src");
  const escape = markdown.utils.escapeHtml;
  const alt = escape(token.content || "Image");
  if (/^https?:\/\//i.test(src)) return '<a href="' + escape(src) + '">' + alt + '</a>';
  if (!src || /^[a-z][a-z0-9+.-]*:|^[\/\\]/i.test(src)) return alt;
  const url = new URL("jam-image://preview/");
  try { url.searchParams.set("path", decodeURIComponent(src.split(/[?#]/)[0])); }
  catch { return alt; }
  return '<img loading="lazy" src="' + escape(url.href) + '" alt="' + alt + '">';
};

let library;
function loadMermaid() {
  if (!library) {
    library = new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = "mermaid.min.js";
      script.onload = () => resolve(window.mermaid);
      script.onerror = () => reject(new Error("Unable to load Mermaid"));
      document.head.append(script);
    });
  }
  return library;
}

let generation = 0;
async function renderMarkdown(source, dark, path) {
  const version = ++generation;
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
  const main = document.querySelector("main");
  main.innerHTML = markdown.render(source);
  const base = new URL("jam-file://preview/");
  base.pathname = path;
  const headings = new Map();
  for (const heading of main.querySelectorAll("h1,h2,h3,h4,h5,h6")) {
    const slug = heading.textContent.toLowerCase().trim().replace(/[^\p{L}\p{N}_ -]/gu, "").replace(/ /g, "-");
    let key = slug;
    for (let n = 1; headings.has(key); n++) key = slug + "-" + n;
    headings.set(key, heading);
  }
  for (const link of main.querySelectorAll("a[href]")) {
    const href = link.getAttribute("href");
    if (href.startsWith("#")) {
      link.addEventListener("click", event => {
        event.preventDefault();
        try { headings.get(decodeURIComponent(href.slice(1)))?.scrollIntoView(); } catch {}
      });
    } else if (!/^[a-z][a-z0-9+.-]*:|^\/\//i.test(href)) {
      link.href = new URL(href, base).href;
    }
  }
  for (const [index, code] of [...main.querySelectorAll("pre > code.language-mermaid")].entries()) {
    if (version !== generation) return;
    const figure = document.createElement("figure");
    const pre = code.parentElement;
    try {
      const mermaid = await loadMermaid();
      if (version !== generation) return;
      mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: dark ? "dark" : "default",
        suppressErrorRendering: true, htmlLabels: false });
      const { svg } = await mermaid.render("diagram-" + version + "-" + index, code.textContent);
      figure.innerHTML = svg;
      pre.replaceWith(figure);
    } catch {
      pre.replaceWith(figure);
      const caption = document.createElement("figcaption");
      caption.textContent = "Unable to render this Mermaid diagram.";
      figure.append(caption, pre);
    }
  }
}
