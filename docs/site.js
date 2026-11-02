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
