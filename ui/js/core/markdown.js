// context-capsule — ui: the little Markdown a capsule uses, read into blocks and inline pieces for the reading view.
// No HTML is made here: the page builds elements from these (so nothing in a capsule can run or style the page).

/**
 * Blocks of a Markdown body: [{ type: "h", level, text } | { type: "p", text } | { type: "ul" | "ol", items: [text] }
 * | { type: "quote", text } | { type: "code", text }]. Front matter must already be removed.
 */
export function blocks(md) {
  const lines = String(md).replace(/\r\n/g, "\n").split("\n");
  const out = [];
  let para = null;
  let list = null;
  const endPara = () => {
    if (para) out.push({ type: "p", text: para.join(" ") });
    para = null;
  };
  const endList = () => {
    if (list) out.push(list);
    list = null;
  };
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const line = raw.trim();
    if (line.startsWith("```")) {
      endPara();
      endList();
      const code = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith("```"); i++) code.push(lines[i]);
      out.push({ type: "code", text: code.join("\n") });
      continue;
    }
    if (!line) {
      endPara();
      endList();
      continue;
    }
    const head = /^(#{1,6})\s+(.*?)\s*#*$/.exec(line);
    if (head) {
      endPara();
      endList();
      out.push({ type: "h", level: head[1].length, text: head[2] });
      continue;
    }
    const bullet = /^[-*+]\s+(.*)$/.exec(line);
    const num = /^\d+[.)]\s+(.*)$/.exec(line);
    if (bullet || num) {
      endPara();
      const type = bullet ? "ul" : "ol";
      if (list && list.type !== type && !/^\s/.test(raw)) endList();
      if (!list) list = { type, items: [] };
      list.items.push((bullet || num)[1]);
      continue;
    }
    if (line.startsWith(">")) {
      endPara();
      endList();
      const text = line.replace(/^>\s?/, "");
      const last = out[out.length - 1];
      if (last?.type === "quote") last.text += " " + text;
      else out.push({ type: "quote", text });
      continue;
    }
    if (list && /^\s/.test(raw)) {
      // A wrapped list item.
      list.items[list.items.length - 1] += " " + line;
      continue;
    }
    endList();
    (para ??= []).push(line);
  }
  endPara();
  endList();
  return out;
}

const INLINE = /(\*\*|__)(.+?)\1|`([^`]+)`|\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)|(https?:\/\/[^\s)>\]]+)|(?<![\p{L}\p{N}*])[*_](?![\s*_])(.+?)(?<![\s*_])[*_](?![\p{L}\p{N}])/gu;

/** Inline pieces: [{ t: "text" | "b" | "i" | "code" | "link", text, url? }]. Links are http(s) only. */
export function inline(text) {
  const s = String(text);
  const out = [];
  let at = 0;
  for (const m of s.matchAll(INLINE)) {
    if (m.index > at) out.push({ t: "text", text: s.slice(at, m.index) });
    if (m[2] != null) out.push({ t: "b", text: m[2] });
    else if (m[3] != null) out.push({ t: "code", text: m[3] });
    else if (m[4] != null) out.push({ t: "link", text: m[4], url: m[5] });
    else if (m[6] != null) {
      let url = m[6];
      let tail = "";
      while (/[.,;:!?]$/.test(url)) {
        tail = url.slice(-1) + tail;
        url = url.slice(0, -1);
      }
      out.push({ t: "link", text: url, url });
      if (tail) out.push({ t: "text", text: tail });
    } else out.push({ t: "i", text: m[7] });
    at = m.index + m[0].length;
  }
  if (at < s.length) out.push({ t: "text", text: s.slice(at) });
  return out;
}
