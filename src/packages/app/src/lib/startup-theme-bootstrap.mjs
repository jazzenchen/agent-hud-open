// Runs at document creation, before the page's modules and stylesheets.
function initializeAppearance(appearance) {
  const preference =
    appearance === "light" || appearance === "dark" ? appearance : "system";
  const theme =
    preference === "system"
      ? matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : preference;
  const apply = () => {
    const root = document.documentElement;
    if (!root) return false;
    root.dataset.initialAppearance = preference;
    root.dataset.theme = theme;
    root.classList.toggle("dark", theme === "dark");
    return true;
  };
  if (!apply()) {
    const observer = new MutationObserver(() => {
      if (apply()) observer.disconnect();
    });
    observer.observe(document, { childList: true, subtree: true });
  }
}
