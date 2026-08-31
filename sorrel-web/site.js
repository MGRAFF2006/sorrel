// Progressive enhancement: sticky header state + persisted theme toggle.

const header = document.querySelector(".site-header");

if (header) {
  const updateHeader = () => {
    header.classList.toggle("is-scrolled", window.scrollY > 12);
  };

  updateHeader();
  window.addEventListener("scroll", updateHeader, { passive: true });
}

const navToggle = document.querySelector(".nav-toggle");
const nav = header?.querySelector(".nav");

if (header && navToggle && nav) {
  const setNavOpen = (open) => {
    header.classList.toggle("is-nav-open", open);
    navToggle.setAttribute("aria-expanded", String(open));
    const label = open ? "Close navigation" : "Open navigation";
    navToggle.setAttribute("aria-label", label);
    navToggle.setAttribute("title", label);
  };

  header.classList.add("has-nav-toggle");
  setNavOpen(false);

  navToggle.addEventListener("click", () => {
    setNavOpen(!header.classList.contains("is-nav-open"));
  });

  nav.addEventListener("click", (event) => {
    if (event.target.closest?.("a")) setNavOpen(false);
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && header.classList.contains("is-nav-open")) {
      setNavOpen(false);
      navToggle.focus();
    }
  });

  const desktop = window.matchMedia("(min-width: 721px)");
  desktop.addEventListener("change", (event) => {
    if (event.matches) setNavOpen(false);
  });
}

const root = document.documentElement;
const toggle = document.querySelector(".theme-toggle");

if (toggle) {
  const apply = (theme) => {
    root.setAttribute("data-theme", theme);
    const label = theme === "light" ? "Switch to dark theme" : "Switch to light theme";
    toggle.setAttribute("aria-label", label);
    toggle.setAttribute("title", label);
  };

  // Initial sync (inline head script already set the attribute; keep the label in step).
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
