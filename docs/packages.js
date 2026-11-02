(() => {
  const REGISTRY = "https://raw.githubusercontent.com/collide2954/schematic-registry/main";
  const REPO = "https://github.com/collide2954/schematic-registry";
  const host = document.getElementById("packages");
  if (!host) return;

  function read(text) {
    let i = 0;
    const skip = () => {
      for (;;) {
        while (i < text.length && /[\s,]/.test(text[i])) i++;
        if (text[i] !== ";") return;
        while (i < text.length && text[i] !== "\n") i++;
      }
    };
    const inside = (close, item) => {
      i++;
      for (skip(); text[i] !== close; skip()) {
        if (i >= text.length) throw new Error(`missing ${close}`);
        item();
      }
      i++;
    };
    const value = () => {
      skip();
      const c = text[i];
      if (c === undefined) throw new Error("unexpected end");
      if (c === "#") { i++; return value(); }
      if (c === "{") {
        const map = {};
        inside("}", () => { const key = value(); map[String(key)] = value(); });
        return map;
      }
      if (c === "[" || c === "(") {
        const items = [];
        inside(c === "[" ? "]" : ")", () => items.push(value()));
        return items;
      }
      if (c === '"') {
        let s = "";
        for (i++; text[i] !== '"'; i++) {
          if (i >= text.length) throw new Error("missing quote");
          if (text[i] === "\\") { i++; s += text[i] === "n" ? "\n" : text[i] === "t" ? "\t" : text[i]; } else s += text[i];
        }
        i++;
        return s;
      }
      const start = i;
      while (i < text.length && !/[\s,\[\]{}()"]/.test(text[i])) i++;
      const token = text.slice(start, i);
      if (!token) throw new Error(`unexpected ${c}`);
      if (token[0] === ":") return token.slice(1);
      if (token === "nil") return null;
      if (token === "true" || token === "false") return token === "true";
      const n = Number(token);
      return Number.isNaN(n) ? token : n;
    };
    return value();
  }

  const newest = (a, b) => {
    const pa = a.split(".").map(Number), pb = b.split(".").map(Number);
    for (let k = 0; k < Math.max(pa.length, pb.length); k++) {
      const d = (pb[k] || 0) - (pa[k] || 0);
      if (d) return d;
    }
    return b.localeCompare(a);
  };

  const el = (tag, attrs, ...children) => {
    const node = document.createElement(tag);
    for (const [name, value] of Object.entries(attrs || {})) node.setAttribute(name, value);
    for (const child of children) if (child) node.append(child);
    return node;
  };

  const sourceOf = (pkg, version) => {
    const github = /^https:\/\/github\.com\/([^/]+\/[^/]+?)(?:\.git)?\/?$/.exec(pkg.git || "");
    if (!github) return pkg.git || REPO;
    const ref = version && (version.tag || version.commit);
    const path = pkg.subdir ? `/${pkg.subdir}` : "";
    return ref ? `https://github.com/${github[1]}/tree/${ref}${path}` : `https://github.com/${github[1]}`;
  };

  const card = (name, pkg) => {
    const versions = Object.keys(pkg.versions || {}).sort(newest);
    const latest = versions[0];
    const add = el("p", { class: "add" }, el("code", null, `schematic add ${name}`));
    return el("article", { class: "package" },
      el("h3", null, el("a", { href: sourceOf(pkg, pkg.versions?.[latest]) }, name), latest && el("span", { class: "version" }, latest), pkg.license && el("span", { class: "license" }, pkg.license)),
      el("p", null, pkg.description || ""),
      add,
      versions.length > 1 && el("p", { class: "versions" }, `Earlier: ${versions.slice(1).join(", ")}`),
    );
  };

  const text = async (path) => {
    const response = await fetch(`${REGISTRY}/${path}`);
    if (!response.ok) throw new Error(`${path}: ${response.status}`);
    return response.text();
  };

  (async () => {
    try {
      const names = read(await text("index.scm")).map((entry) => (typeof entry === "string" ? entry : entry.name)).sort();
      const cards = await Promise.all(names.map(async (name) => card(name, read(await text(`packages/${name}.scm`)))));
      host.replaceChildren(...(cards.length ? cards : [el("p", { class: "note" }, "Nothing is listed yet.")]));
    } catch {
      host.replaceChildren(el("p", { class: "note" }, "The registry could not be read just now. ", el("a", { href: `${REPO}/tree/main/packages` }, "Browse it on GitHub"), "."));
    }
  })();
})();
