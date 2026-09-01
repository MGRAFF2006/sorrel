// Minimal Markdown renderer for Sorrel docs (no dependencies).
// Supports headings, paragraphs, lists, tables, fenced code, links, inline code, bold.

const ALLOWED = new Set([
  "STATUS.md",
  "GETTING_STARTED.md",
  "ARCHITECTURE.md",
  "DEVELOPMENT.md",
  "RELEASE.md",
  "CHANGELOG.md",
]);

const contentEl = document.getElementById("docs-content");
const navLinks = document.querySelectorAll(".docs-nav a[data-doc]");

function escapeHtml(text) {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function inlineFormat(text, references = new Map()) {
  let out = escapeHtml(text);
  out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
  out = out.replace(/\[([^\]]+)\]/g, (match, label) => {
    const href = references.get(label.toLowerCase());
    return href ? `<a href="${escapeHtml(href)}">${label}</a>` : match;
  });
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>");
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  return out;
}

function renderMarkdown(src) {
  const references = new Map();
  const withoutComments = src.replace(/<!--[\s\S]*?-->/g, "");
  const lines = withoutComments
    .replace(/\r\n/g, "\n")
    .split("\n")
    .filter((line) => {
      const definition = /^\[([^\]]+)\]:\s+(\S+)\s*$/.exec(line);
      if (!definition) return true;
      references.set(definition[1].toLowerCase(), definition[2]);
      return false;
    });
  const html = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.startsWith("```")) {
      const lang = escapeHtml(line.slice(3).trim());
      const code = [];
      i += 1;
      while (i < lines.length && !lines[i].startsWith("```")) {
        code.push(lines[i]);
        i += 1;
      }
      i += 1;
      html.push(
        `<pre class="docs-code"${lang ? ` data-lang="${lang}"` : ""}><code>${escapeHtml(code.join("\n"))}</code></pre>`,
      );
      continue;
    }

    if (/^\|.+\|$/.test(line) && i + 1 < lines.length && /^\|[\s:|-]+\|$/.test(lines[i + 1])) {
      const rows = [];
      while (i < lines.length && /^\|.+\|$/.test(lines[i])) {
        rows.push(lines[i]);
        i += 1;
      }
      const parseRow = (row) =>
        row
          .replace(/^\||\|$/g, "")
          .split("|")
          .map((cell) => cell.trim());
      const header = parseRow(rows[0]);
      const body = rows.slice(2).map(parseRow);
      html.push("<table><thead><tr>");
      header.forEach((cell) => html.push(`<th>${inlineFormat(cell, references)}</th>`));
      html.push("</tr></thead><tbody>");
      body.forEach((row) => {
        html.push("<tr>");
        row.forEach((cell) => html.push(`<td>${inlineFormat(cell, references)}</td>`));
        html.push("</tr>");
      });
      html.push("</tbody></table>");
      continue;
    }

    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      html.push(`<h${level}>${inlineFormat(heading[2], references)}</h${level}>`);
      i += 1;
      continue;
    }

    if (/^[-*]\s+/.test(line)) {
      html.push("<ul>");
      while (i < lines.length && /^[-*]\s+/.test(lines[i])) {
        const item = [lines[i].replace(/^[-*]\s+/, "")];
        i += 1;
        while (i < lines.length && lines[i].trim() && !/^[-*]\s+/.test(lines[i])) {
          item.push(lines[i].trim());
          i += 1;
        }
        html.push(`<li>${inlineFormat(item.join(" "), references)}</li>`);
      }
      html.push("</ul>");
      continue;
    }

    if (/^\d+\.\s+/.test(line)) {
      html.push("<ol>");
      while (i < lines.length && /^\d+\.\s+/.test(lines[i])) {
        const item = [lines[i].replace(/^\d+\.\s+/, "")];
        i += 1;
        while (i < lines.length && lines[i].trim() && !/^\d+\.\s+/.test(lines[i])) {
          item.push(lines[i].trim());
          i += 1;
        }
        html.push(`<li>${inlineFormat(item.join(" "), references)}</li>`);
      }
      html.push("</ol>");
      continue;
    }

    if (!line.trim()) {
      i += 1;
      continue;
    }

    const para = [line];
    i += 1;
    while (
      i < lines.length &&
      lines[i].trim() &&
      !lines[i].startsWith("#") &&
      !lines[i].startsWith("```") &&
      !/^[-*]\s+/.test(lines[i]) &&
      !/^\d+\.\s+/.test(lines[i]) &&
      !/^\|.+\|$/.test(lines[i])
    ) {
      para.push(lines[i]);
      i += 1;
    }
    html.push(`<p>${inlineFormat(para.join(" "), references)}</p>`);
  }

  return html.join("\n");
}

function inferLanguage(code, declared) {
  if (declared) return declared.toLowerCase();
  if (/^\s*[{[]/.test(code)) return "json";
  if (/^\s*(version|workflows|jobs|command|needs|env):/m.test(code)) return "yaml";
  if (/^\s*\$/m.test(code) || /\b(npm|cargo|sorrel|docker)\s/m.test(code)) return "shell";
  return "text";
}

function highlightLine(line) {
  const tokens = /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\`(?:\\.|[^\`\\])*\`|\/\/.*$|#.*$|--?[\w-]+|\b(?:true|false|null|let|const|fn|pub|struct|enum|impl|match|if|else|use|mod|version|workflows|jobs|command|needs|env|secretRef)\b|\b\d+(?:\.\d+)?\b|\$(?=\s))/g;
  let html = "";
  let cursor = 0;

  for (const match of line.matchAll(tokens)) {
    const token = match[0];
    html += escapeHtml(line.slice(cursor, match.index));
    let kind = "number";
    if (token.startsWith("#") || token.startsWith("//")) kind = "comment";
    else if (/^["'\`]/.test(token)) kind = "string";
    else if (token === "$") kind = "prompt";
    else if (token.startsWith("-")) kind = "flag";
    else if (/^[a-zA-Z]/.test(token)) kind = "keyword";
    html += `<span class="syntax-${kind}">${escapeHtml(token)}</span>`;
    cursor = match.index + token.length;
  }

  return html + escapeHtml(line.slice(cursor));
}

async function copyText(text, button, successLabel) {
  try {
    await navigator.clipboard.writeText(text);
    const previous = button.textContent;
    const previousLabel = button.getAttribute("aria-label");
    button.textContent = "Copied";
    button.setAttribute("aria-label", successLabel);
    window.setTimeout(() => {
      button.textContent = previous;
      button.setAttribute("aria-label", previousLabel);
    }, 1400);
  } catch {
    button.textContent = "Copy failed";
  }
}

function enhanceCodeBlocks(root = document) {
  root.querySelectorAll("pre:not(.is-enhanced)").forEach((pre) => {
    const code = pre.querySelector("code");
    if (!code) return;

    const raw = code.textContent.replace(/\n$/, "");
    const language = inferLanguage(raw, pre.dataset.lang);
    const toolbar = document.createElement("div");
    const label = document.createElement("span");
    const copy = document.createElement("button");
    toolbar.className = "code-toolbar";
    label.textContent = language;
    copy.type = "button";
    copy.className = "copy-block";
    copy.textContent = "Copy";
    copy.setAttribute("aria-label", "Copy code block");
    copy.addEventListener("click", () => copyText(raw, copy, "Code block copied"));
    toolbar.append(label, copy);

    code.textContent = "";
    raw.split("\n").forEach((line, index) => {
      const row = document.createElement("span");
      const number = document.createElement("span");
      const source = document.createElement("span");
      const lineCopy = document.createElement("button");
      row.className = "code-line";
      number.className = "line-number";
      number.textContent = String(index + 1);
      number.setAttribute("aria-hidden", "true");
      source.className = "line-source";
      source.innerHTML = highlightLine(line) || " ";
      lineCopy.type = "button";
      lineCopy.className = "copy-line";
      lineCopy.textContent = "Copy";
      lineCopy.setAttribute("aria-label", `Copy line ${index + 1}`);
      lineCopy.addEventListener("click", () =>
        copyText(line, lineCopy, `Line ${index + 1} copied`),
      );
      row.append(number, source, lineCopy);
      code.append(row);
    });

    pre.classList.add("is-enhanced");
    pre.insertBefore(toolbar, code);
  });
}

function assignHeadingIds(root) {
  const used = new Set();
  root.querySelectorAll("h1, h2, h3").forEach((heading) => {
    const base = heading.textContent
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "section";
    let id = base;
    let suffix = 2;
    while (used.has(id) || (document.getElementById(id) && document.getElementById(id) !== heading)) {
      id = `${base}-${suffix}`;
      suffix += 1;
    }
    used.add(id);
    heading.id = id;
  });
}

function buildPageToc(root) {
  document.querySelector(".page-toc")?.remove();
  document.querySelector(".has-page-toc")?.classList.remove("has-page-toc");
  const headings = [...root.querySelectorAll("h2, h3")];
  if (headings.length < 2) return;

  assignHeadingIds(root);
  const nav = document.createElement("nav");
  const title = document.createElement("p");
  const list = document.createElement("ol");
  nav.className = "page-toc";
  nav.setAttribute("aria-label", "On this page");
  title.textContent = "On this page";
  headings.forEach((heading) => {
    const item = document.createElement("li");
    const link = document.createElement("a");
    item.className = `toc-${heading.tagName.toLowerCase()}`;
    link.href = `#${heading.id}`;
    link.textContent = heading.textContent;
    item.append(link);
    list.append(item);
  });
  nav.append(title, list);

  if (contentEl) {
    contentEl.parentElement.append(nav);
  } else {
    const section = root.parentElement;
    section.classList.add("has-page-toc");
    section.append(nav);
  }
}

function setActive(doc) {
  navLinks.forEach((link) => {
    const active = link.getAttribute("data-doc") === doc;
    link.classList.toggle("is-active", active);
    if (active) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  });
}

async function loadDoc(name, historyMode = "replace") {
  const doc = ALLOWED.has(name) ? name : "STATUS.md";
  setActive(doc);
  if (contentEl) {
    contentEl.setAttribute("aria-busy", "true");
    contentEl.innerHTML = '<p class="muted">Loading…</p>';
  }
  try {
    const response = await fetch(`./${doc}`, { headers: { accept: "text/plain,text/markdown,*/*" } });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const text = await response.text();
    if (contentEl) {
      contentEl.innerHTML = renderMarkdown(text);
      contentEl.removeAttribute("aria-busy");
      enhanceCodeBlocks(contentEl);
      buildPageToc(contentEl);
    }
    document.title = `${doc.replace(/\.md$/, "").replace(/_/g, " ")} — Sorrel docs`;
    const url = new URL(window.location.href);
    url.searchParams.set("doc", doc);
    if (historyMode === "push") history.pushState({ doc }, "", url);
    else if (historyMode === "replace") history.replaceState({ doc }, "", url);
  } catch (error) {
    if (contentEl) {
      contentEl.removeAttribute("aria-busy");
      contentEl.innerHTML = `<p class="muted">Could not load <code>${escapeHtml(doc)}</code>: ${escapeHtml(error.message)}. Open the <a href="./${escapeHtml(doc)}">raw markdown</a> instead.</p>`;
    }
  }
}

if (contentEl) {
  const params = new URLSearchParams(window.location.search);
  loadDoc(params.get("doc") || "STATUS.md");

  navLinks.forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      loadDoc(link.getAttribute("data-doc"), "push");
    });
  });

  window.addEventListener("popstate", () => {
    const next = new URLSearchParams(window.location.search).get("doc");
    loadDoc(next || "STATUS.md", "none");
  });
} else {
  enhanceCodeBlocks();
  const article = document.querySelector(".doc-body");
  if (article) buildPageToc(article);
}
