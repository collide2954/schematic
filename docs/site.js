(() => {
  const toc = document.querySelector("nav.toc");
  if (!toc) return;
  const headings = Array.from(document.querySelectorAll("main h2"));
  if (headings.length < 3) {
    toc.remove();
    return;
  }
  const list = document.createElement("ul");
  for (const heading of headings) {
    if (!heading.id) heading.id = heading.textContent.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-");
    const link = document.createElement("a");
    link.href = `#${heading.id}`;
    link.textContent = heading.textContent;
    const item = document.createElement("li");
    item.append(link);
    list.append(item);
  }
  toc.append(list);
  toc.hidden = false;
})();

(() => {
  if (!location.pathname.endsWith("reference.html")) return;
  const main = document.querySelector("main");
  const title = main.querySelector("h1");
  if (!title) return;
  const box = document.createElement("input");
  box.type = "search";
  box.className = "filter";
  box.placeholder = "Name";
  box.setAttribute("aria-label", "Filter by name");
  title.after(box);
  const sections = Array.from(main.querySelectorAll("h2")).map((heading) => {
    const nodes = [];
    for (let node = heading.nextElementSibling; node && node.tagName !== "H2"; node = node.nextElementSibling) nodes.push(node);
    const rows = nodes.flatMap((node) => Array.from(node.querySelectorAll("tbody tr")));
    const entry = document.querySelector(`nav.toc a[href="#${heading.id}"]`)?.parentElement;
    return { heading, nodes, rows, entry };
  });
  box.addEventListener("input", () => {
    const query = box.value.trim().toLowerCase();
    for (const section of sections) {
      let shown = 0;
      for (const row of section.rows) {
        const hit = !query || row.cells[0].textContent.toLowerCase().includes(query);
        row.hidden = !hit;
        if (hit) shown++;
      }
      const hide = Boolean(query) && shown === 0;
      section.heading.hidden = hide;
      if (section.entry) section.entry.hidden = hide;
      for (const node of section.nodes) node.hidden = hide || (Boolean(query) && node.tagName !== "TABLE");
    }
  });
})();
