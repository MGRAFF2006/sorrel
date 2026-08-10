// Minimal Markdown renderer for Sorrel docs (no dependencies).
// Supports headings, paragraphs, lists, tables, fenced code, links, inline code, bold.

const ALLOWED = new Set(["STATUS.md", "GETTING_STARTED.md"]);

const contentEl = document.getElementById("docs-content");
const navLinks = document.querySelectorAll(".docs-nav a[data-doc]");

function escapeHtml(text) {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function inlineFormat(text) {
  let out = escapeHtml(text);
  out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>");
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  return out;
}

function renderMarkdown(src) {
  const lines = src.replace(/\r\n/g, "\n").split("\n");
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
      header.forEach((cell) => html.push(`<th>${inlineFormat(cell)}</th>`));
      html.push("</tr></thead><tbody>");
      body.forEach((row) => {
        html.push("<tr>");
        row.forEach((cell) => html.push(`<td>${inlineFormat(cell)}</td>`));
        html.push("</tr>");
      });
      html.push("</tbody></table>");
      continue;
    }

    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      html.push(`<h${level}>${inlineFormat(heading[2])}</h${level}>`);
      i += 1;
      continue;
    }

    if (/^[-*]\s+/.test(line)) {
      html.push("<ul>");
      while (i < lines.length && /^[-*]\s+/.test(lines[i])) {
        html.push(`<li>${inlineFormat(lines[i].replace(/^[-*]\s+/, ""))}</li>`);
        i += 1;
      }
      html.push("</ul>");
      continue;
    }

    if (/^\d+\.\s+/.test(line)) {
      html.push("<ol>");
      while (i < lines.length && /^\d+\.\s+/.test(lines[i])) {
        html.push(`<li>${inlineFormat(lines[i].replace(/^\d+\.\s+/, ""))}</li>`);
        i += 1;
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
    html.push(`<p>${inlineFormat(para.join(" "))}</p>`);
  }

  return html.join("\n");
}

function setActive(doc) {
  navLinks.forEach((link) => {
    const active = link.getAttribute("data-doc") === doc;
    link.classList.toggle("is-active", active);
    if (active) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  });
}

async function loadDoc(name) {
  const doc = ALLOWED.has(name) ? name : "STATUS.md";
  setActive(doc);
  if (contentEl) {
    contentEl.innerHTML = '<p class="muted">Loading…</p>';
  }
  try {
    const response = await fetch(`./${doc}`, { headers: { accept: "text/plain,text/markdown,*/*" } });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const text = await response.text();
    if (contentEl) contentEl.innerHTML = renderMarkdown(text);
    document.title = `${doc.replace(/\.md$/, "").replace(/_/g, " ")} — Sorrel docs`;
    const url = new URL(window.location.href);
    url.searchParams.set("doc", doc);
    history.replaceState(null, "", url);
  } catch (error) {
    if (contentEl) {
      contentEl.innerHTML = `<p class="muted">Could not load <code>${escapeHtml(doc)}</code>: ${escapeHtml(error.message)}. Open the <a href="./${escapeHtml(doc)}">raw markdown</a> instead.</p>`;
    }
  }
}

const params = new URLSearchParams(window.location.search);
loadDoc(params.get("doc") || "STATUS.md");

navLinks.forEach((link) => {
  link.addEventListener("click", (event) => {
    event.preventDefault();
    loadDoc(link.getAttribute("data-doc"));
  });
});
