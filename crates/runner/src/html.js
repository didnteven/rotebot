(() => {
  // Reads ordinary web pages as data: no DOM, no network, only the text the host fetched.
  const names = {amp: "&", lt: "<", gt: ">", quot: "\"", apos: "'", nbsp: " "};
  const page = (html) => {
    if (typeof html !== "string") throw new TypeError("macroscope.html takes the page text from macroscope.source(name)");
    return html;
  };
  const decode = (s) => s.replace(/&(#[xX][0-9a-fA-F]+|#\d+|[a-zA-Z]+);/g, (m, e) => {
    if (e[0] === "#") {
      const n = e[1] === "x" || e[1] === "X" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
      return n > 0 && n < 0x110000 ? String.fromCodePoint(n) : m;
    }
    const v = names[e.toLowerCase()];
    return v === undefined ? m : v;
  });
  const visible = (html) => html
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<(script|style|noscript|svg|template|head)\b[\s\S]*?<\/\1\s*>/gi, " ");
  const flat = (s) => decode(s.replace(/<[^>]*>/g, " ")).replace(/[\s\u00a0]+/g, " ").trim();
  const text = (html) => decode(visible(page(html))
      .replace(/<br\s*\/?>/gi, "\n")
      .replace(/<\/?(p|div|li|ul|ol|tr|table|h[1-6]|section|article|header|footer|main|nav|dt|dd|blockquote|pre)\b[^>]*>/gi, "\n")
      .replace(/<[^>]*>/g, " "))
    .replace(/[ \t\r\f\v\u00a0]+/g, " ")
    .replace(/ *\n[ \n]*/g, "\n")
    .trim();
  const tables = (html) => {
    const out = [];
    const body = visible(page(html));
    for (const t of body.match(/<table\b[\s\S]*?<\/table\s*>/gi) || []) {
      let headers = [];
      const rows = [];
      for (const r of t.match(/<tr\b[\s\S]*?(?=<tr\b|<\/tbody|<\/thead|<\/tfoot|<\/table)/gi) || []) {
        const cells = (r.match(/<t[hd]\b[\s\S]*?(?=<t[hd]\b|<\/tr|$)/gi) || []).map(flat);
        if (cells.length === 0 || cells.every((c) => c === "")) continue;
        if (headers.length === 0 && rows.length === 0 && /<th\b/i.test(r)) headers = cells;
        else rows.push(cells);
      }
      if (headers.length || rows.length) out.push({headers, rows});
    }
    return out;
  };
  const attr = (attrs, name) => {
    const m = new RegExp("\\b" + name + "\\s*=\\s*(\"([^\"]*)\"|'([^']*)'|([^\\s>]+))", "i").exec(attrs);
    return m ? decode(m[2] ?? m[3] ?? m[4] ?? "") : null;
  };
  const links = (html) => {
    const out = [];
    const re = /<a\b([^>]*)>([\s\S]*?)<\/a\s*>/gi;
    const body = visible(page(html));
    let m;
    while ((m = re.exec(body)) !== null) {
      const href = attr(m[1], "href");
      if (href !== null) out.push({text: flat(m[2]), href});
    }
    return out;
  };
  // Data many sites embed for their own scripts: JSON script blocks and JSON-LD.
  const json = (html) => {
    const out = [];
    const re = /<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi;
    let m;
    while ((m = re.exec(page(html))) !== null) {
      const type = (attr(m[1], "type") || "").toLowerCase();
      if (!type.includes("json") && attr(m[1], "id") !== "__NEXT_DATA__") continue;
      try { out.push(JSON.parse(m[2])); } catch (_) { /* not JSON after all */ }
    }
    return out;
  };
  // Headings in page order, so the lead story or main item can be told from menus and sections.
  const headings = (html) => {
    const out = [];
    const re = /<h([1-6])\b[^>]*>([\s\S]*?)<\/h\1\s*>/gi;
    const body = visible(page(html));
    let m;
    while ((m = re.exec(body)) !== null) {
      const text = flat(m[2]);
      if (text) out.push({level: Number(m[1]), text});
    }
    return out;
  };
  const title = (html) => {
    const m = /<title\b[^>]*>([\s\S]*?)<\/title\s*>/i.exec(page(html));
    return m ? flat(m[1]) : "";
  };
  return {text, tables, links, json, title, headings};
})
