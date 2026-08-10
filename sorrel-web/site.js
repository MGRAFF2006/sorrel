// Progressive enhancement: sticky header state + persisted theme toggle.

const header = document.querySelector(".site-header");

if (header) {
  const updateHeader = () => {
    header.classList.toggle("is-scrolled", window.scrollY > 12);
  };

  updateHeader();
  window.addEventListener("scroll", updateHeader, { passive: true });
}

const root = document.documentElement;
const toggle = document.querySelector(".theme-toggle");

if (toggle) {
  const apply = (theme) => {
    root.setAttribute("data-theme", theme);
    toggle.setAttribute("aria-pressed", String(theme === "light"));
  };

  // Initial sync (inline head script already set the attribute; keep aria in step).
  apply(root.getAttribute("data-theme") === "light" ? "light" : "dark");

  toggle.addEventListener("click", () => {
    const next = root.getAttribute("data-theme") === "light" ? "dark" : "light";
    apply(next);
    try {
      localStorage.setItem("sorrel-theme", next);
    } catch (e) {
      /* storage unavailable; theme still applies for the session */
    }
  });
}
