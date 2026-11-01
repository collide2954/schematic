(() => {
  const root = document.documentElement;
  const toggle = document.querySelector(".theme-toggle");
  if (toggle) {
    toggle.addEventListener("click", () => {
      const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      const current = root.dataset.theme || (dark ? "dark" : "light");
      const next = current === "dark" ? "light" : "dark";
      root.dataset.theme = next;
      try { localStorage.setItem("theme", next); } catch (_) {}
    });
  }

  const copyButton = (text) => {
    const button = document.createElement("button");
    button.className = "copy";
    button.type = "button";
    button.textContent = "Copy";
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(text());
        button.textContent = "Copied";
        setTimeout(() => { button.textContent = "Copy"; }, 1200);
      } catch (_) {
        button.textContent = "Select and copy";
      }
    });
    return button;
  };
  window.copyButton = copyButton;

  document.querySelectorAll("pre").forEach((pre) => {
    if (pre.classList.contains("play-out") || pre.classList.contains("out") || pre.closest(".example") || pre.closest(".play")) return;
    pre.appendChild(copyButton(() => pre.querySelector("code")?.textContent ?? pre.textContent));
  });
})();
