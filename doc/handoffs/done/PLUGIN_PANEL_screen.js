/* ranma: plugin screens, tooltips and badges (card c138).
   Every scene is drawn into a grid of cells holding theme role names, the way
   SETTINGS_PANEL_screen.js draws the settings panel. The drawing is the design:
   ranma's tests compare against the text this script prints. */

const CELLW = 9;
const ROWH = 18;

const BS = {
  rounded: { tl: "╭", tr: "╮", bl: "╰", br: "╯", h: "─", v: "│", lt: "├", rt: "┤", tt: "┬", bt: "┴", x: "┼" },
  plain:   { tl: "┌", tr: "┐", bl: "└", br: "┘", h: "─", v: "│", lt: "├", rt: "┤", tt: "┬", bt: "┴", x: "┼" },
  thick:   { tl: "┏", tr: "┓", bl: "┗", br: "┛", h: "━", v: "┃", lt: "┣", rt: "┫", tt: "┳", bt: "┻", x: "╋" },
  double:  { tl: "╔", tr: "╗", bl: "╚", br: "╝", h: "═", v: "║", lt: "╠", rt: "╣", tt: "╦", bt: "╩", x: "╬" },
  ascii:   { tl: "+", tr: "+", bl: "+", br: "+", h: "-", v: "|", lt: "+", rt: "+", tt: "+", bt: "+", x: "+" },
  none:    { tl: " ", tr: " ", bl: " ", br: " ", h: " ", v: " ", lt: " ", rt: " ", tt: " ", bt: " ", x: " " }
};

const THEMES = {
  default: {
    bg: "#1e1e2e", fg: "#cdd6f4", surface: "#313244",
    border_active: "#89b4fa", border_inactive: "#45475a", border_floating: "#f5c2e7",
    bar_bg: "default", bar_fg: "#cdd6f4", bar_dim: "#6c7086", bar_accent: "#89b4fa", bar_urgent: "#f38ba8",
    mode_fg: "#1e1e2e", mode_bg: "#f38ba8",
    ws_active_fg: "#1e1e2e", ws_active_bg: "#89b4fa", ws_occupied: "#cdd6f4", ws_empty: "#6c7086", ws_urgent: "#f38ba8",
    picker_selected_fg: "#1e1e2e", picker_selected_bg: "#89b4fa",
    toast_fg: "#cdd6f4", toast_bg: "#313244",
    red: "#f38ba8", green: "#a6e3a1", yellow: "#f9e2af", blue: "#89b4fa", magenta: "#cba6f7", cyan: "#94e2d5", gray: "#6c7086", orange: "#fab387"
  },
  matugen: {
    bg: "#151d19", fg: "#dce5df", surface: "#26312b",
    border_active: "#8bd6b4", border_inactive: "#3c4842", border_floating: "#a8cbe0",
    bar_bg: "default", bar_fg: "#dce5df", bar_dim: "#86948c", bar_accent: "#8bd6b4", bar_urgent: "#ffb4ab",
    mode_fg: "#0f1f26", mode_bg: "#a8cbe0",
    ws_active_fg: "#00382a", ws_active_bg: "#8bd6b4", ws_occupied: "#dce5df", ws_empty: "#86948c", ws_urgent: "#ffb4ab",
    picker_selected_fg: "#00382a", picker_selected_bg: "#8bd6b4",
    toast_fg: "#dce5df", toast_bg: "#26312b",
    red: "#ffb4ab", green: "#a3d39c", yellow: "#e8c983", blue: "#9ccaff", magenta: "#d7bde4", cyan: "#8fd0d6", gray: "#86948c", orange: "#f2b88a"
  },
  latte: {
    bg: "#eff1f5", fg: "#4c4f69", surface: "#dce0e8",
    border_active: "#1e66f5", border_inactive: "#bcc0cc", border_floating: "#8839ef",
    bar_bg: "default", bar_fg: "#4c4f69", bar_dim: "#7c7f93", bar_accent: "#1e66f5", bar_urgent: "#d20f39",
    mode_fg: "#eff1f5", mode_bg: "#8839ef",
    ws_active_fg: "#eff1f5", ws_active_bg: "#1e66f5", ws_occupied: "#4c4f69", ws_empty: "#8c8fa1", ws_urgent: "#d20f39",
    picker_selected_fg: "#eff1f5", picker_selected_bg: "#1e66f5",
    toast_fg: "#4c4f69", toast_bg: "#dce0e8",
    red: "#d20f39", green: "#40a02b", yellow: "#df8e1d", blue: "#1e66f5", magenta: "#8839ef", cyan: "#179299", gray: "#8c8fa1", orange: "#fe640b"
  }
};

/* ---------- cells ---------- */

function hexRgb(h) {
  h = h.replace("#", "");
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}
function rgbHex(r) {
  return "#" + r.map(function (v) { return Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0"); }).join("");
}
function mix(a, b, t) {
  const A = hexRgb(a), B = hexRgb(b);
  return rgbHex([0, 1, 2].map(function (i) { return A[i] + (B[i] - A[i]) * t; }));
}
function res(T, c) {
  if (!c || c === "bg") return T.bg;
  if (c[0] === "#") return c;
  const v = T[c];
  if (v === undefined) return T.fg;
  if (v === "default") return T.bg;
  return v;
}
function len(s) { return Array.from(String(s)).length; }
function trunc(s, n) {
  const a = Array.from(String(s));
  if (a.length <= n) return String(s);
  if (n <= 0) return "";
  return a.slice(0, n - 1).join("") + "…";
}
// A link keeps its host and its end; the middle gives way.
function truncMid(s, n) {
  const a = Array.from(String(s));
  if (a.length <= n) return String(s);
  if (n <= 1) return n === 1 ? "…" : "";
  const head = Math.ceil((n - 1) / 2), tail = n - 1 - head;
  return a.slice(0, head).join("") + "…" + (tail ? a.slice(a.length - tail).join("") : "");
}
function isSafe(ch) {
  const c = ch.codePointAt(0);
  return (c >= 0x20 && c < 0x7f) || (c >= 0x2500 && c <= 0x259f);
}

class Scr {
  constructor(W, H) {
    this.W = W; this.H = H; this.clip = null;
    this.cells = [];
    for (let i = 0; i < W * H; i++) this.cells.push({ ch: " ", fg: "fg", bg: "bg", b: false, it: false, u: false, r: false, f: 0 });
  }
  set(x, y, ch, st) {
    if (x < 0 || y < 0 || x >= this.W || y >= this.H) return;
    const k = this.clip;
    if (k && (x < k.x || x >= k.x + k.w || y < k.y || y >= k.y + k.h)) return;
    const c = this.cells[y * this.W + x];
    st = st || {};
    c.ch = ch;
    if (st.fg !== undefined) c.fg = st.fg;
    if (st.bg !== undefined) c.bg = st.bg;
    c.b = !!st.b; c.it = !!st.it; c.u = !!st.u; c.r = !!st.r; c.f = st.f || 0;
  }
  put(x, y, s, st) {
    const a = Array.from(String(s));
    for (let i = 0; i < a.length; i++) this.set(x + i, y, a[i], st);
    return x + a.length;
  }
  fill(x, y, w, h, st, ch) {
    for (let yy = y; yy < y + h; yy++) for (let xx = x; xx < x + w; xx++) this.set(xx, yy, ch || " ", st);
  }
  box(x, y, w, h, bs, st) {
    const B = BS[bs];
    for (let i = 1; i < w - 1; i++) { this.set(x + i, y, B.h, st); this.set(x + i, y + h - 1, B.h, st); }
    for (let j = 1; j < h - 1; j++) { this.set(x, y + j, B.v, st); this.set(x + w - 1, y + j, B.v, st); }
    this.set(x, y, B.tl, st); this.set(x + w - 1, y, B.tr, st);
    this.set(x, y + h - 1, B.bl, st); this.set(x + w - 1, y + h - 1, B.br, st);
  }
  // Reverse a cell in place (the mock's way of showing where the pointer is).
  rev(x, y) {
    if (x < 0 || y < 0 || x >= this.W || y >= this.H) return;
    this.cells[y * this.W + x].r = true;
  }
}

function toLines(s, T) {
  const out = [];
  for (let y = 0; y < s.H; y++) {
    const spans = [];
    let cur = null;
    for (let x = 0; x < s.W; x++) {
      const c = s.cells[y * s.W + x];
      const safe = isSafe(c.ch);
      let bg = res(T, c.bg);
      let fg = res(T, c.fg);
      if (c.f) fg = mix(fg, bg, c.f);
      if (c.r) { const t = fg; fg = bg; bg = t; }
      const key = fg + bg + c.b + c.it + c.u;
      if (cur && safe && cur.safe && cur.key === key) { cur.t += c.ch; cur.n++; }
      else { cur = { t: c.ch, n: 1, key: key, safe: safe, fg: fg, bg: bg, b: c.b, it: c.it, u: c.u }; spans.push(cur); }
    }
    out.push({ spans: spans.map(function (p) {
      return { t: p.t, w: p.n * CELLW, fg: p.fg, bg: p.bg, fw: p.b ? "700" : "400", fs: p.it ? "italic" : "normal", td: p.u ? "underline" : "none" };
    }) });
  }
  return out;
}
function toText(s) {
  const out = [];
  for (let y = 0; y < s.H; y++) {
    let row = "";
    for (let x = 0; x < s.W; x++) row += s.cells[y * s.W + x].ch;
    out.push(row);
  }
  return out.join("\n");
}

function wrap(text, w, max) {
  const words = String(text).split(" ");
  const lines = [];
  let cur = "";
  words.forEach(function (wd) {
    if (!cur.length) cur = wd;
    else if (len(cur) + 1 + len(wd) <= w) cur += " " + wd;
    else { lines.push(cur); cur = wd; }
  });
  if (cur.length) lines.push(cur);
  if (lines.length > max) {
    const keep = lines.slice(0, max);
    keep[max - 1] = trunc(keep[max - 1] + " …", w);
    return keep;
  }
  return lines.map(function (l) { return trunc(l, w); });
}

/* ---------- roles ----------
   A plugin names one of four roles for a value, a mark, a badge or a line of
   text; ranma maps them onto the theme. Nothing else is open to it. */

const ROLE = { normal: "toast_fg", dim: "bar_dim", accent: "bar_accent", urgent: "bar_urgent" };
function roleSt(role, sel, strong) {
  role = role || "normal";
  if (sel) return role === "dim" ? { fg: "picker_selected_fg", f: 0.4 } : { fg: "picker_selected_fg", b: role !== "normal" || !!strong };
  return { fg: ROLE[role] || "toast_fg", b: role === "urgent" || !!strong };
}

/* ---------- blocks ----------
   heading  { k:"heading", text, tag?, count? }
   row      { k:"row", id, name, mark?:{t,role}, note?, value?, select?:false, keys?, detail?:[blocks] }
            value: { shape:"text", t, role } | { shape:"choice", v } | { shape:"toggle", v }
                 | { shape:"slider", frac, t } | { shape:"swatch", hex } | { shape:"field", v }
   text     { k:"text", t, role?, strong?, max? }
   facts    { k:"facts", items:[[label, value, {role, strong}?]] }
   progress { k:"progress", label?, frac, num? }
   log      { k:"log", lines:[string | [[text, "hit"|"num"|"strong"]]], n?, at? }
   sep      { k:"sep" }       space { k:"space" } */

function valueSegs(v, CW, sel) {
  const segs = [];
  if (!v) return segs;
  const P = function (t, st) { segs.push([t, st]); };
  const compact = CW < 28, sp = compact ? "" : " ";
  const base = sel ? { fg: "picker_selected_fg" } : { fg: "toast_fg" };
  const dim = sel ? { fg: "picker_selected_fg", f: 0.4 } : { fg: "bar_dim" };
  const acc = sel ? { fg: "picker_selected_fg", b: true } : { fg: "bar_accent" };
  const sh = v.shape || "text";
  if (sh === "text") P(v.t, roleSt(v.role, sel));
  else if (sh === "choice") { P("‹" + sp, acc); P(String(v.v), base); P(sp + "›", acc); }
  else if (sh === "toggle") { P("[" + sp, dim); P(v.v ? "on" : "off", v.v ? Object.assign({}, acc, { b: true }) : base); P(sp + "]", dim); }
  else if (sh === "slider") {
    const sw = CW >= 60 ? 17 : CW >= 40 ? 7 : CW >= 34 ? 5 : 0;
    if (sw) {
      const k = Math.round(v.frac * (sw - 1));
      for (let i = 0; i < sw; i++) {
        if (i < k) P("━", sel ? { fg: "picker_selected_fg" } : { fg: "bar_accent" });
        else if (i === k) P("●", sel ? { fg: "picker_selected_fg", b: true } : { fg: "toast_fg", b: true });
        else P("─", dim);
      }
      P(" ", base);
    }
    if (!compact && len(v.t) < 4) P(" ".repeat(4 - len(v.t)), base);
    P("‹" + sp, acc); P(v.t, base); P(sp + "›", acc);
  } else if (sh === "swatch") {
    P("[" + sp, dim); P("██", { fg: v.hex }); P(" " + v.hex, base); P(sp + "]", dim);
  } else if (sh === "field") {
    const maxIn = Math.max(4, CW - (CW >= 60 ? 44 : 22));
    P("[" + sp, dim); P(trunc("\"" + v.v + "\"", maxIn), base); P(sp + "]", dim);
  }
  return segs;
}

function drawRow(s, x0, y, CW, r, sel, ctx) {
  ctx = ctx || {};
  const wide = CW >= 60;
  const base = sel ? { fg: "picker_selected_fg" } : { fg: "toast_fg" };
  if (sel) s.fill(x0 - 1, y, CW + 2, 1, { bg: "picker_selected_bg" });
  if (r.select !== false) s.put(x0, y, sel ? "›" : " ", { fg: sel ? "picker_selected_fg" : "bar_accent", b: true });
  const segs = valueSegs(r.value, CW, sel);
  const vw = segs.reduce(function (a, sg) { return a + len(sg[0]); }, 0);
  const vx = x0 + CW - vw;
  const mk = r.mark ? r.mark.t : "";
  const mkW = mk ? len(mk) + 1 : 0;
  const showNote = wide && r.note;
  const room = vx - (x0 + 2) - 2 - mkW;
  const nameMax = Math.max(1, showNote ? Math.min(24 - mkW, room) : room);
  const nm = trunc(r.name, nameMax);
  const q = (ctx.query || "").toLowerCase();
  const mi = q ? r.name.toLowerCase().indexOf(q) : -1;
  const na = Array.from(nm);
  for (let i = 0; i < na.length; i++) {
    const hit = mi >= 0 && i >= mi && i < mi + q.length && na[i] !== "…";
    s.set(x0 + 2 + i, y, na[i], Object.assign({}, base, { u: hit, b: hit }));
  }
  if (mk) s.put(x0 + 2 + na.length + 1, y, mk, roleSt(r.mark.role, sel, r.mark.role !== "normal"));
  if (showNote) {
    const nx = x0 + 28, nmax = vx - 2 - nx;
    if (nmax > 3) s.put(nx, y, trunc(r.note, nmax), sel ? { fg: "picker_selected_fg", f: 0.4 } : { fg: "bar_dim" });
  }
  let x = vx;
  segs.forEach(function (sg) { x = s.put(x, y, sg[0], sg[1]); });
}

function drawHead(s, x0, y, CW, h, hch) {
  const cnt = h.count != null ? String(h.count) : "";
  const tagW = h.tag ? len(h.tag) + 1 : 0;
  const nameMax = Math.max(1, CW - tagW - (cnt ? len(cnt) + 1 : 0) - 4);
  let x = s.put(x0, y, trunc(h.text, nameMax), { fg: "bar_accent", b: true });
  if (h.tag) x = s.put(x + 1, y, h.tag, { fg: "bar_dim" });
  const end = cnt ? x0 + CW - len(cnt) - 1 : x0 + CW;
  for (x = x + 1; x < end; x++) s.set(x, y, hch, { fg: "bar_dim" });
  if (cnt) s.put(x0 + CW - len(cnt), y, cnt, { fg: "bar_dim" });
}

// label value · label value: pairs go from the end, then the labels go.
function drawFacts(s, x, y, W, items) {
  const wOf = function (arr, lab) {
    return arr.reduce(function (a, it, i) { return a + (i ? 3 : 0) + (lab && it[0] ? len(it[0]) + 1 : 0) + len(it[1]); }, 0);
  };
  const its = items.slice();
  while (its.length > 1 && wOf(its, true) > W) its.pop();
  const lab = wOf(its, true) <= W;
  let cx = x;
  its.forEach(function (it, i) {
    if (i) cx = s.put(cx, y, " · ", { fg: "bar_dim" });
    if (lab && it[0]) cx = s.put(cx, y, it[0] + " ", { fg: "bar_dim" });
    const o = it[2] || {};
    cx = s.put(cx, y, trunc(it[1], Math.max(1, x + W - cx)), roleSt(o.role, false, o.strong));
  });
}

// The slider without its arrows: label, bar, number. Below 5 cells the bar goes.
function drawProgress(s, x, y, W, b) {
  const lab = b.label || "", num = b.num || "";
  const bw = W - len(lab) - len(num) - (lab ? 1 : 0) - (num ? 1 : 0);
  let cx = x;
  if (lab) cx = s.put(cx, y, trunc(lab, Math.max(1, W - len(num) - 1)), { fg: "toast_fg" }) + 1;
  if (bw >= 5) {
    const k = Math.round(b.frac * (bw - 1));
    for (let i = 0; i < bw; i++) {
      if (i < k) s.set(cx + i, y, "━", { fg: "bar_accent" });
      else if (i === k) s.set(cx + i, y, "●", { fg: "toast_fg", b: true });
      else s.set(cx + i, y, "─", { fg: "bar_dim" });
    }
  }
  if (num) s.put(x + W - len(num), y, num, { fg: "toast_fg" });
}

// A log line is cut on the right, no ellipsis: it is a tail of something longer.
function drawLog(s, x, y, W, ln, hot) {
  const segs = typeof ln === "string" ? [[ln]] : ln;
  const base = hot ? { fg: "toast_fg" } : { fg: "bar_dim" };
  const old = s.clip;
  s.clip = { x: x, y: y, w: Math.max(0, W), h: 1 };
  let cx = x;
  segs.forEach(function (sg) {
    let st = base;
    if (sg[1] === "hit") st = { fg: "toast_fg", b: true, u: true };
    else if (sg[1] === "num") st = { fg: "bar_dim" };
    else if (sg[1] === "strong") st = { fg: "toast_fg", b: true };
    cx = s.put(cx, y, sg[0], st);
  });
  s.clip = old;
}

function logWindow(b, n) {
  const L = b.lines.length;
  n = Math.min(n, L);
  let start = L - n;
  if (b.at != null) start = Math.max(0, Math.min(b.at - Math.floor((n - 1) / 2), L - n));
  return { start: start, n: n };
}

// Blocks to one-row items. ind: 2 in the list (under the names), 0 in the detail.
function layoutBlocks(blocks, CW, ind, ctx) {
  const out = [];
  (blocks || []).forEach(function (b) {
    if (b.k === "heading") out.push({ head: b, draw: function (s, x, y) { drawHead(s, x, y, CW, b, ctx.hch); } });
    else if (b.k === "row") out.push({ row: b, draw: function (s, x, y, on) { drawRow(s, x, y, CW, b, on, ctx); } });
    else if (b.k === "text") wrap(b.t, CW - ind, b.max || 4).forEach(function (ln) {
      out.push({ draw: function (s, x, y) { s.put(x + ind, y, ln, roleSt(b.role, false, b.strong)); } });
    });
    else if (b.k === "facts") out.push({ draw: function (s, x, y) { drawFacts(s, x + ind, y, CW - ind, b.items); } });
    else if (b.k === "progress") out.push({ draw: function (s, x, y) { drawProgress(s, x + ind, y, CW - ind, b); } });
    else if (b.k === "log") {
      const wdw = logWindow(b, b.n || b.lines.length);
      for (let i = 0; i < wdw.n; i++) {
        const j = wdw.start + i;
        out.push({ log: b, draw: function (s, x, y) { drawLog(s, x + ind, y, CW - ind, b.lines[j], b.at === j); } });
      }
    }
    else if (b.k === "sep") out.push({ draw: function (s, x, y) { for (let i = 0; i < CW - ind; i++) s.set(x + ind + i, y, ctx.hch, { fg: "border_inactive" }); } });
    else if (b.k === "space") out.push({ draw: function () {} });
  });
  return out;
}

// The detail area keeps its facts: logs give up lines first.
function fitDetail(blocks, D, CW, ctx) {
  const bl = blocks.map(function (b) { return Object.assign({}, b); });
  let items = layoutBlocks(bl, CW, 0, ctx);
  let guard = 200;
  while (items.length > D && guard--) {
    let big = null;
    bl.forEach(function (b) { if (b.k === "log") { const n = b.n || b.lines.length; if (n > 1 && (!big || n > (big.n || big.lines.length))) big = b; } });
    if (!big) break;
    big.n = Math.min(big.n || big.lines.length, big.lines.length) - 1;
    items = layoutBlocks(bl, CW, 0, ctx);
  }
  return items.slice(0, D);
}

/* ---------- keys ---------- */

const RESERVED = ["↑", "↓", "j", "k", "tab", "/", "esc", "?"];

function valueKeys(v) {
  if (!v) return [];
  if (v.shape === "slider") return [["←→", "step"]];
  if (v.shape === "choice") return [["←→", "choose"]];
  if (v.shape === "toggle") return [["←→", "flip"]];
  if (v.shape === "field") return [["enter", "edit"]];
  return [];
}
function footKeys(S, row, wide, grouped) {
  const k = [];
  if (row) valueKeys(row.value).forEach(function (p) { k.push(p); });
  if (row && row.keys) row.keys.forEach(function (p) { k.push(p); });
  (S.keys || []).forEach(function (p) { k.push(p); });
  if (wide) {
    if (S.filter) k.push(["/", "filter"]);
    if (grouped) k.push(["tab", "next group"]);
    if (row && !(S.keys || []).some(function (p) { return p[0] === "space"; })) k.push(["space", "peek"]);
  }
  return k;
}
// In order, until one does not fit; "? keys" is never dropped.
function putKeys(s, x, y, keys, maxX) {
  const tw = 2 + len("? keys");
  let first = true;
  for (let i = 0; i < keys.length; i++) {
    const kv = keys[i];
    const need = (first ? 0 : 2) + len(kv[0]) + 1 + len(kv[1]);
    if (x + need + (first ? tw - 2 : tw) > maxX) break;
    if (!first) x += 2;
    x = s.put(x, y, kv[0], { fg: "bar_accent", b: true });
    x = s.put(x + 1, y, kv[1], { fg: "bar_dim" });
    first = false;
  }
  if (!first) x += 2;
  x = s.put(x, y, "?", { fg: "bar_accent", b: true });
  return s.put(x + 1, y, "keys", { fg: "bar_dim" });
}
function putEdgeKeys(s, xr, y, pairs, bg) {
  const bl = [[" ", 0]];
  pairs.forEach(function (p, i) { if (i) bl.push([" · ", 0]); bl.push([p[0], 1]); bl.push([" " + p[1], 0]); });
  bl.push([" ", 0]);
  let bx = xr - bl.reduce(function (a, p) { return a + len(p[0]); }, 0);
  bl.forEach(function (p) { bx = s.put(bx, y, p[0], p[1] ? { fg: "bar_accent", bg: bg, b: true } : { fg: "bar_dim", bg: bg }); });
}

/* ---------- the screen ---------- */

function bottomKeys(S) {
  if (S.bottom) return S.bottom;
  const b = [];
  if (S.options) b.push(["o", "options"]);
  b.push(["esc", "close"]);
  return b;
}

function drawScreen(s, P) {
  const x = P.x, y = P.y, w = P.w, h = P.h, bs = P.bs, B = BS[bs], S = P.scr;
  const none = bs === "none";
  const bst = { fg: "mode_bg", bg: none ? "toast_bg" : "bg" };
  const rst = { fg: "border_inactive", bg: "toast_bg" };
  const hch = bs === "ascii" ? "-" : "─";
  s.fill(x, y, w, h, { bg: "toast_bg", fg: "toast_fg" });
  s.box(x, y, w, h, bs, bst);
  const heads = S.body.filter(function (b) { return b.k === "heading"; });
  const wide = w >= 90;
  const useIndex = wide && heads.length >= 2;
  const IX = useIndex ? 22 : 0;
  const sepX = x + 1 + IX;
  const x0 = useIndex ? sepX + 2 : x + 2;
  const CW = (x + w - 2) - x0;
  const ctx = { hch: hch, query: S.query || "" };
  const items = layoutBlocks(S.body, CW, 2, ctx);
  let selIdx = -1;
  if (items.some(function (it) { return it.row && it.row.select !== false; })) {
    selIdx = items.findIndex(function (it) { return it.row && it.row.id === S.sel; });
    if (selIdx < 0) selIdx = items.findIndex(function (it) { return it.row && it.row.select !== false; });
  }
  const selRow = selIdx >= 0 ? items[selIdx].row : null;

  const qY = y + 1, r1 = y + 2, listTop = y + 3;
  const footY = y + h - 2, rule2 = footY - 1;
  const body = rule2 - listTop;
  let D = 0;
  if (selRow && selRow.detail && selRow.detail.length) {
    D = Math.min(Math.max(S.detail || 3, Math.floor(body / 5)), Math.floor(body * 0.4));
    if (body - D - 1 < 5) D = 0;
  }
  const rule1 = D ? rule2 - D - 1 : rule2;
  const L = rule1 - listTop;

  const hRule = function (yy, j) {
    for (let i = x + 1; i < x + w - 1; i++) s.set(i, yy, B.h, rst);
    s.set(x, yy, B.lt, bst); s.set(x + w - 1, yy, B.rt, bst);
    if (useIndex && j) s.set(sepX, yy, j, rst);
  };
  hRule(r1, B.x);
  if (D) hRule(rule1, B.bt);
  hRule(rule2, D ? null : B.bt);
  if (useIndex) {
    for (let yy = y + 1; yy < rule1; yy++) if (yy !== r1) s.set(sepX, yy, B.v, rst);
    s.set(sepX, y, B.tt, bst);
  }

  // title, status, bottom keys
  s.put(x + 2, y, " " + S.title + " ", { fg: "toast_fg", bg: bst.bg, b: true });
  if (S.status) {
    const t = " " + S.status.t + " ";
    s.put(x + w - 2 - len(t), y, t, Object.assign(roleSt(S.status.role, false, true), { bg: bst.bg }));
  }
  putEdgeKeys(s, x + w - 2, y + h - 1, bottomKeys(S), bst.bg);

  // the first row: the filter, or the plugin's subtitle; a count on the right
  if (S.filter) {
    if (S.query) {
      let qx = s.put(x0, qY, "/", { fg: "bar_accent", b: true });
      qx = s.put(qx + 1, qY, S.query, { fg: "toast_fg", b: true });
      if (S.filtering) s.set(qx, qY, " ", { bg: "toast_fg" });
    } else {
      const qx = s.put(x0, qY, "/", { fg: "bar_dim", b: true });
      s.put(qx + 1, qY, "filter", { fg: "bar_dim" });
    }
  } else if (S.subtitle) s.put(x0, qY, trunc(S.subtitle, CW - len(S.count || "") - 2), { fg: "bar_dim" });
  if (S.count) s.put(x0 + CW - len(S.count), qY, S.count, { fg: "bar_dim" });

  // the group index (wide)
  if (useIndex) {
    s.put(x + 2, qY, "Groups", { fg: "bar_dim" });
    let curHead = null;
    for (let i = selIdx; i >= 0; i--) if (items[i].head) { curHead = items[i].head; break; }
    heads.forEach(function (hd, i) {
      const gy = listTop + i;
      const cur = hd === curHead;
      s.put(x + 2, gy, cur ? "›" : " ", { fg: "bar_accent", b: true });
      s.put(x + 4, gy, trunc(hd.text, 13), cur ? { fg: "toast_fg", b: true } : { fg: "toast_fg" });
      const cnt = hd.count != null ? String(hd.count) : "";
      s.put(sepX - 1 - len(cnt), gy, cnt, { fg: "bar_dim" });
    });
  }

  // the list
  let off;
  if (selIdx >= 0) {
    let hi = selIdx;
    while (hi > 0 && !items[hi].head) hi--;
    off = hi;
    if (selIdx - off >= L) off = selIdx - L + 3;
  } else off = S.scroll || 0;
  off = Math.max(0, Math.min(off, Math.max(0, items.length - L)));
  if (!items.length) s.put(x0, listTop, S.empty || "nothing here", { fg: "bar_dim" });
  for (let i = 0; i < L; i++) {
    const it = items[off + i];
    if (!it) break;
    it.draw(s, x0, listTop + i, off + i === selIdx);
  }
  if (items.length > L) {
    const th = Math.max(1, Math.round(L * L / items.length));
    const tp = Math.round(off / (items.length - L) * (L - th));
    const tch = bs === "thick" ? "█" : bs === "ascii" ? "#" : bs === "none" ? "▐" : "┃";
    for (let i = 0; i < th; i++) s.set(x + w - 1, listTop + tp + i, tch, { fg: "toast_fg", bg: bst.bg });
  }

  // the detail of the selected row
  if (D) fitDetail(selRow.detail, D, CW, ctx).forEach(function (it, i) { it.draw(s, x0, rule1 + 1 + i, false); });

  putKeys(s, x0, footY, footKeys(S, selRow, wide, heads.length >= 2), x0 + CW);

  if (P.help) drawKeysCard(s, x + Math.floor((w - 40) / 2), y + 2, S, bs);
  return selRow;
}

/* ---------- the keys card ---------- */

function ranmaKeys(S) {
  const rows = S.body.filter(function (b) { return b.k === "row" && b.select !== false; });
  const k = [[rows.length ? "↑↓ j k" : "↑↓ j k", rows.length ? "move" : "scroll"]];
  if (S.body.filter(function (b) { return b.k === "heading"; }).length >= 2) k.push(["tab", "next group"]);
  if (S.filter) k.push(["/", S.filter === "plugin" ? "search again" : "filter by name"]);
  if (rows.length && !(S.keys || []).some(function (p) { return p[0] === "space"; })) k.push(["space", "peek at the panes"]);
  if (S.options) k.push(["o", S.title + " options"]);
  k.push(["esc", "close"]);
  return k;
}
function drawKeysCard(s, x, y, S, bs) {
  const pk = S.card || [];
  const rk = ranmaKeys(S);
  const w = 40, h = 3 + pk.length + rk.length;
  const hch = bs === "ascii" ? "-" : "─";
  s.fill(x, y, w, h, { bg: "bg", fg: "toast_fg" });
  s.box(x, y, w, h, bs, { fg: "bar_accent", bg: "bg" });
  s.put(x + 2, y, " keys ", { fg: "toast_fg", bg: "bg", b: true });
  let yy = y + 1;
  pk.forEach(function (k) {
    s.put(x + 2, yy, k[0], { fg: "bar_accent", b: true });
    s.put(x + 11, yy, trunc(k[1], w - 13 - (k[2] ? len(k[2]) + 1 : 0)), { fg: "toast_fg" });
    if (k[2]) s.put(x + w - 2 - len(k[2]), yy, k[2], { fg: "bar_dim" });
    yy++;
  });
  let px = s.put(x + 2, yy, "ranma ", { fg: "bar_dim" });
  for (; px < x + w - 2; px++) s.set(px, yy, hch, { fg: "border_inactive" });
  yy++;
  rk.forEach(function (k) {
    s.put(x + 2, yy, k[0], { fg: "bar_accent", b: true });
    s.put(x + 11, yy, k[1], { fg: "toast_fg" });
    yy++;
  });
  s.put(x + 2, y + h - 1, " any key ", { fg: "bar_dim", bg: "bg" });
}

/* ---------- peek ---------- */

function drawPeek(s, bs, S, row, W, H) {
  const y = H - 4;
  const bst = { fg: "mode_bg", bg: bs === "none" ? "toast_bg" : "bg" };
  s.fill(0, y, W, 3, { bg: "toast_bg", fg: "toast_fg" });
  s.box(0, y, W, 3, bs, bst);
  s.put(2, y, " " + S.title + " · peek ", { fg: "toast_fg", bg: bst.bg, b: true });
  if (S.status) { const t = " " + S.status.t + " "; s.put(W - 2 - len(t), y, t, Object.assign(roleSt(S.status.role, false, true), { bg: bst.bg })); }
  drawRow(s, 2, y + 1, W - 4, row, true, {});
  const pairs = [["space", "back"]];
  if (row.keys && row.keys[0]) pairs.push(row.keys[0]);
  pairs.push(["esc", "close"]);
  putEdgeKeys(s, W - 2, y + 2, pairs, bst.bg);
}

/* ---------- badges on a pane's title edge ----------
   One badge per plugin per pane, in plugin load order, after the title.
   As the edge narrows: words go, the title is cut (never below 4 letters),
   calm badges go (dim, then normal, then accent), the title's name goes,
   urgent badges go last; the index stays. */

const CALM = { dim: 0, normal: 1, accent: 2, urgent: 3 };
const LADDER = [
  "everything",
  "badge words go; glyphs stay",
  "the title is cut, to 4 letters at least",
  "calm badges go: dim, then normal, then accent",
  "the title's name goes; its index stays",
  "urgent badges go last",
  "only the index"
];
function titleRun(p, room, hch, bfg) {
  const tst = { fg: bfg, b: !!p.focus };
  let mark = p.sync ? "⇉ " : "";
  let name = p.name || "";
  let words = true;
  let badges = (p.badges || []).slice();
  const make = function () {
    const pcs = [[" " + mark + p.idx + (name ? " " + name : "") + " ", tst]];
    badges.forEach(function (b) {
      const st = { fg: ROLE[b.role] || "toast_fg", b: b.role === "urgent" };
      pcs.push([hch + " ", { fg: bfg }]);
      pcs.push([b.g, st]);
      if (words && b.t) pcs.push([" " + b.t, st]);
      pcs.push([" ", { fg: bfg }]);
    });
    return pcs;
  };
  const fits = function () { return make().reduce(function (a, pc) { return a + len(pc[0]); }, 0) <= room; };
  const dropCalmest = function (keepUrgent) {
    let best = -1;
    badges.forEach(function (b, i) {
      if (keepUrgent && b.role === "urgent") return;
      if (best < 0 || CALM[b.role] <= CALM[badges[best].role]) best = i;
    });
    if (best < 0) return false;
    badges.splice(best, 1);
    return true;
  };
  if (fits()) return { pieces: make(), step: 0 };
  words = false;
  if (fits()) return { pieces: make(), step: 1 };
  const full = name;
  for (let n = len(full) - 1; n >= 5; n--) { name = trunc(full, n); if (fits()) return { pieces: make(), step: 2 }; }
  while (!fits() && dropCalmest(true)) {}
  if (fits()) return { pieces: make(), step: 3 };
  name = "";
  if (fits()) return { pieces: make(), step: 4 };
  while (!fits() && dropCalmest(false)) {}
  if (fits()) return { pieces: make(), step: 5 };
  mark = "";
  return { pieces: make(), step: 6 };
}

function drawPanes(s, panes, bs) {
  panes.forEach(function (p) {
    const bfg = p.focus ? "border_active" : "border_inactive";
    s.box(p.x, p.y, p.w, p.h, bs, { fg: bfg, bg: "bg" });
    s.clip = { x: p.x + 1, y: p.y + 1, w: p.w - 2, h: p.h - 2 };
    if (p.draw) p.draw(s, p.x + 1, p.y + 1, p.w - 2, p.h - 2);
    s.clip = { x: p.x + 1, y: p.y, w: p.w - 2, h: 1 };
    let cx = p.x + 1;
    titleRun(p, p.w - 3, BS[bs].h, bfg).pieces.forEach(function (pc) { cx = s.put(cx, p.y, pc[0], Object.assign({ bg: "bg" }, pc[1])); });
    s.clip = null;
  });
}

/* ---------- tooltips ----------
   Anchored to a cell. Below it when it fits, else above; it never covers the
   anchor's row. Its left edge starts 3 cells before the anchor and is pushed
   left at the right edge. A tick on the near border points at the anchor. */

function drawTooltip(s, A, tip, area, bs) {
  const B = BS[bs];
  const lines = [];
  if (tip.title) lines.push([[tip.title, { fg: "toast_fg", b: true }]]);
  (tip.lines || []).forEach(function (l) { lines.push(l); });
  const maxW = Math.min(52, area.w);
  const inner = Math.min(maxW - 4, lines.reduce(function (a, l) { return Math.max(a, l.reduce(function (b, sg) { return b + len(sg[0]); }, 0)); }, 0));
  const w = inner + 4, h = lines.length + 2;
  const below = A.y + 1 + h <= area.y + area.h;
  const ty = below ? A.y + 1 : A.y - h;
  let tx = A.x - 3;
  if (tx + w > area.x + area.w) tx = area.x + area.w - w;
  if (tx < area.x) tx = area.x;
  const bst = { fg: "bar_dim", bg: bs === "none" ? "toast_bg" : "bg" };
  s.fill(tx, ty, w, h, { bg: "toast_bg", fg: "toast_fg" });
  s.box(tx, ty, w, h, bs, bst);
  if (bs !== "none") s.set(A.x, below ? ty : ty + h - 1, below ? B.bt : B.tt, bst);
  lines.forEach(function (l, i) {
    const old = s.clip;
    s.clip = { x: tx + 2, y: ty + 1 + i, w: inner, h: 1 };
    let cx = tx + 2;
    l.forEach(function (sg) { cx = s.put(cx, ty + 1 + i, sg[0], sg[1]); });
    s.clip = old;
  });
  return { x: tx, y: ty, w: w, h: h, below: below, flipped: tx + 3 !== A.x };
}
function keyLine(pairs) {
  const out = [];
  pairs.forEach(function (p, i) {
    if (i) out.push(["  ", {}]);
    out.push([p[0], { fg: "bar_accent", b: true }]);
    out.push([" " + p[1], { fg: "bar_dim" }]);
  });
  return out;
}

/* ---------- the bar ---------- */

function drawBar(s, y, W, chip, opt) {
  opt = opt || {};
  s.fill(0, y, W, 1, { fg: "bar_fg", bg: "bar_bg" });
  let x = chip ? s.put(0, y, " " + chip + " ", { fg: "mode_fg", bg: "mode_bg", b: true }) + 2 : 0;
  x = s.put(x, y, " 1:code ", { fg: "ws_active_fg", bg: "ws_active_bg", b: true });
  x = s.put(x, y, opt.narrow ? " 2 " : " 2:web ", { fg: "ws_occupied" });
  s.put(x, y, " 3 ", { fg: "ws_empty" });
  const r1 = opt.narrow ? "" : "cpu 12%  mem 41%  ", r2 = "16:36 ";
  if (r1) s.put(W - len(r1) - len(r2), y, r1, { fg: "bar_dim" });
  s.put(W - len(r2), y, r2, { fg: "bar_fg" });
}

/* ---------- what is in the panes ---------- */

function segLines(lines, fromTop) {
  return function (s, x, y, w, h) {
    const start = fromTop ? 0 : Math.max(0, lines.length - h);
    for (let i = start; i < lines.length && i - start < h; i++) {
      let cx = x;
      lines[i].forEach(function (sg) { cx = s.put(cx, y + i - start, sg[0], { fg: sg[1] || "fg", b: !!sg[2], u: sg[3] === "u", it: sg[3] === "i" }); });
    }
  };
}
const T_ = function (t, c, b, a) { return [t, c, b, a]; };

const CODE = [
  "//! Layouts: a tree of splits.", "", "use crate::pane::PaneId;", "",
  "pub fn dwindle(area: Rect, n: usize) -> Vec<Rect> {", "    let mut out = Vec::new();", "    let mut rest = area;",
  "    for i in 0..n {", "        if i + 1 == n {", "            out.push(rest);", "            break;", "        }",
  "        let (a, b) = split(rest, i);", "        out.push(a);", "        rest = b;", "    }", "    out", "}", "",
  "pub fn master(area: Rect, n: usize, ratio: f32) -> Vec<Rect> {", "    let w = (area.w as f32 * ratio).round() as u16;",
  "    let (m, stack) = area.split_x(w);", "    let mut out = vec![m];", "    out.extend(stack.rows(n - 1));", "    out", "}"
];
const KW = /^(pub|fn|let|mut|use|for|in|if|return|break|struct|impl|self|match|else|const|crate)$/;
function rustLine(src) {
  if (/^\s*\/\//.test(src)) return [[src, "gray"]];
  const segs = [];
  const re = /(\s+|[A-Za-z_][A-Za-z0-9_]*|\d+|"[^"]*"|.)/g;
  let m, prevFn = false;
  while ((m = re.exec(src))) {
    const t = m[0];
    let r = "fg";
    if (/^\s+$/.test(t)) r = "fg";
    else if (KW.test(t)) r = "magenta";
    else if (/^\d/.test(t)) r = "orange";
    else if (t[0] === "\"") r = "green";
    else if (/^[A-Z]/.test(t)) r = "yellow";
    else if (prevFn && /^[a-z_]/.test(t)) r = "blue";
    else if (/^[{}()\[\];:,.<>=+\-&|!]$/.test(t)) r = "gray";
    if (!/^\s+$/.test(t)) prevFn = t === "fn";
    segs.push([t, r]);
  }
  return segs;
}
function nvim(s, x, y, w, h) {
  for (let i = 0; i < h - 1 && i < CODE.length; i++) {
    let cx = s.put(x, y + i, String(i + 1).padStart(3) + " ", { fg: i === 8 ? "yellow" : "gray" });
    rustLine(CODE[i]).forEach(function (sg) { cx = s.put(cx, y + i, sg[0], { fg: sg[1] }); });
  }
  for (let i = CODE.length; i < h - 1; i++) s.put(x, y + i, "~", { fg: "blue" });
  s.fill(x, y + h - 1, w, 1, { bg: "surface" });
  const cx = s.put(x, y + h - 1, " NORMAL ", { fg: "bg", bg: "blue", b: true });
  s.put(cx, y + h - 1, " layout.rs ", { fg: "fg", bg: "surface" });
}
const ZSH = segLines([
  [T_("~/src/ranma", "cyan"), T_(" main", "magenta")],
  [T_("❯ ", "green"), T_("git log --oneline -6")],
  [T_("a3f91c2 ", "yellow"), T_("plugin: screens from blocks")],
  [T_("7d02e4b ", "yellow"), T_("picker: headings stay on filter")],
  [T_("c11a9e0 ", "yellow"), T_("theme: toast roles for panels")],
  [T_("58be7d1 ", "yellow"), T_("bar: mode chip for sub-modes")],
  [T_("9b2d6f8 ", "yellow"), T_("doc: plugins in Lua")],
  [T_("0de93b2 ", "yellow"), T_("toast: urgent border")],
  [T_("~/src/ranma", "cyan"), T_(" main", "magenta")],
  [T_("❯ ", "green"), T_("█")]
]);
const CARGO_OK = segLines([
  [T_("$ ", "gray"), T_("cargo test -q")],
  [T_("running 42 tests")],
  [T_("test layout::dwindle ... "), T_("ok", "green")],
  [T_("test layout::master ... "), T_("ok", "green")],
  [T_("test picker::filter ... "), T_("ok", "green")],
  [T_("test theme::inherit ... "), T_("ok", "green")],
  [T_("test render::crop ... "), T_("ok", "green")],
  [T_("")],
  [T_("test result: "), T_("ok", "green"), T_(". 42 passed")],
  [T_("$ ", "gray"), T_("█")]
]);
const CARGO_PANIC = segLines([
  [T_("test paste::upload ... "), T_("ok", "green")],
  [T_("test toast::stack ... "), T_("ok", "green")],
  [T_("thread 'main' panicked at src/layout.rs:88:5:", "red")],
  [T_("attempt to subtract with overflow")],
  [T_("note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace", "gray")],
  [T_("test layout::master ... "), T_("FAILED", "red", true)],
  [T_("")],
  [T_("failures:")],
  [T_("    layout::master")],
  [T_("")],
  [T_("test result: "), T_("FAILED", "red", true), T_(". 41 passed; 1 failed")],
  [T_("error: test failed, to rerun pass `--lib`", "red")],
  [T_("$ ", "gray"), T_("█")]
]);

// What an agent in a pane looks like: a transcript and, at the end, its question.
const API_LINES = [
  [T_("> ", "gray"), T_("the api returns 503 under load; retry it")],
  [T_("")],
  [T_("● ", "fg"), T_("Reading src/client.rs")],
  [T_("● ", "fg"), T_("The client gives up on the first 503.")],
  [T_("  I'll retry with backoff, at most 3 times.")],
  [T_("")],
  [T_("  ⎿ ", "gray"), T_("Edit src/handler.rs  "), T_("+14 −3", "green")],
  [T_("")],
  [T_("Do you want to proceed?", "yellow", true)],
  [T_("❯ ", "blue"), T_("1. Yes")],
  [T_("  2. Yes, and don't ask again")],
  [T_("  3. No, tell it what to do")]
];
const API = segLines(API_LINES);
const WEB = segLines([
  [T_("> ", "gray"), T_("port the settings form to the new api")],
  [T_("")],
  [T_("• ", "fg"), T_("Updated src/settings/form.tsx")],
  [T_("• ", "fg"), T_("Running npm test -- settings")],
  [T_("  ", "fg"), T_("PASS", "green", true), T_(" form.test.tsx (12)")],
  [T_("  ", "fg"), T_("RUNS", "yellow", true), T_(" api.test.tsx")]
]);
const AGENT = function (lines) { return segLines(lines.map(function (l) { return [T_(l[0], l[1] || "fg", l[2])]; })); };

/* ---------- the plugins' screens (what each plugin hands ranma) ---------- */

function agentsScreen(wide) {
  const facts = function (ws, pane, cwd, since) { return { k: "facts", items: [["ws", ws], ["pane", pane], ["", cwd], since] }; };
  const apiLog = [
    "> the api returns 503 under load; retry it",
    "● Reading src/client.rs",
    "● The client gives up on the first 503.",
    "  I'll retry with backoff, at most 3 times.",
    "  ⎿ Edit src/handler.rs  +14 −3",
    "Do you want to proceed?",
    "❯ 1. Yes",
    "  2. Yes, and don't ask again",
    "  3. No, tell it what to do"
  ];
  return {
    title: "agents", chip: "AGENTS", options: true, filter: "names",
    status: { t: "2 need you", role: "urgent" }, count: "5 agents", detail: 4, sel: "api",
    card: [["enter", "jump to its pane"], ["a", "answer it", "waiting"], ["x", "stop it: ctrl+c", "working"], ["r", "run it again", "failed"], ["d", "dismiss", "ended"]],
    body: [
      { k: "heading", text: "Needs you", count: 2 },
      { k: "row", id: "api", name: "api · claude", note: "1:code  ~/src/api", value: { t: "? 12m", role: "urgent" },
        keys: [["enter", "jump"], ["a", "answer"], ["x", "stop"]],
        detail: [{ k: "log", lines: apiLog, at: 5 }, facts("1:code", "1", "~/src/api", ["waiting", "12m", { strong: true }])] },
      { k: "row", id: "infra", name: "infra · gemini", note: "3  ~/src/infra", value: { t: "✗ exit 1", role: "urgent" },
        keys: [["enter", "jump"], ["r", "rerun"], ["d", "dismiss"]],
        detail: [{ k: "log", lines: ["✗ terraform plan failed: provider not configured", "exited 1"] }, facts("3", "2", "~/src/infra", ["ended", "3m ago"])] },
      { k: "heading", text: "Working", count: 2 },
      { k: "row", id: "web", name: "web · codex", note: "1:code  ~/src/web", value: { t: "● 4m", role: "accent" },
        keys: [["enter", "jump"], ["x", "stop"]],
        detail: [{ k: "log", lines: ["• Updated src/settings/form.tsx", "• Running npm test -- settings", "  PASS form.test.tsx (12)", "  RUNS api.test.tsx"] }, facts("1:code", "3", "~/src/web", ["working", "4m"])] },
      { k: "row", id: "ranma", name: "ranma · claude", note: "2:web  ~/src/ranma", value: { t: "● 31m", role: "accent" },
        keys: [["enter", "jump"], ["x", "stop"]],
        detail: [{ k: "log", lines: ["● Running cargo test -q", "  ⎿ 42 passed"] }, facts("2:web", "1", "~/src/ranma", ["working", "31m"])] },
      { k: "heading", text: "Done", count: 1 },
      { k: "row", id: "docs", name: "docs · aider", note: "2:web  ~/src/docs", value: { t: "✓ 2m ago", role: "dim" },
        keys: [["enter", "jump"], ["d", "dismiss"]],
        detail: [{ k: "log", lines: ["Applied edit to doc/CONFIG.md", "Commit 4be1c07 doc: plugin screens"] }, facts("2:web", "2", "~/src/docs", ["done", "2m ago"])] }
    ]
  };
}

function historyScreen() {
  const ctxLines = [
    [["−1206  ", "num"], ["test paste::upload ... ok"]],
    [["−1205  ", "num"], ["test toast::stack ... ok"]],
    [["−1204  ", "num"], ["thread 'main' "], ["panic", "hit"], ["ked at src/layout.rs:88:5:"]],
    [["−1203  ", "num"], ["attempt to subtract with overflow"]],
    [["−1202  ", "num"], ["note: run with `RUST_BACKTRACE=1` environment variable"]]
  ];
  const m = function (id, name, at, n, extra) {
    return { k: "row", id: id, name: name, value: { t: at, role: "dim" }, keys: [["enter", "jump"], ["y", "copy"], ["c", "copy mode"]],
      detail: [{ k: "log", lines: extra || ctxLines, at: 2 }, { k: "facts", items: [["pane", "3 cargo"], ["line", at], ["", n + " of 5"]] }] };
  };
  return {
    title: "history", chip: "HIST", filter: "plugin", query: "panic", count: "5 matches", detail: 6, sel: "m1",
    card: [["enter", "jump: scroll the pane there"], ["y", "copy the line"], ["c", "copy mode at the line"]],
    body: [
      { k: "heading", text: "3 cargo", tag: "scrollback", count: 5 },
      m("m1", "thread 'main' panicked at src/layout.rs:88:5:", "−1204", 1),
      m("m2", "thread 'picker::filter' panicked at src/picker.rs:212:13:", "−940", 2),
      m("m3", "note: panic = \"abort\" in this profile", "−611", 3),
      m("m4", "thread 'tokio-rt' panicked at src/server.rs:57:22:", "−388", 4),
      m("m5", "error: test failed (2 panics)", "−12", 5)
    ]
  };
}

function mediaScreen() {
  return {
    title: "media", chip: "MEDIA", options: true, status: { t: "playing", role: "accent" },
    subtitle: "spotify", count: "1 of 2 players", sel: "vol",
    keys: [["space", "pause"], ["n", "next"], ["p", "previous"]],
    card: [["space", "play or pause"], ["n", "next track"], ["p", "previous track"], ["s", "switch player"]],
    body: [
      { k: "heading", text: "Now playing" },
      { k: "text", t: "Weightless", strong: true },
      { k: "text", t: "Marconi Union · Weightless (Ambient Transmission Vol. 2)", role: "dim", max: 1 },
      { k: "space" },
      { k: "progress", label: "1:42", frac: 0.21, num: "8:09" },
      { k: "space" },
      { k: "heading", text: "Player", count: 4 },
      { k: "row", id: "vol", name: "Volume", value: { shape: "slider", frac: 0.6, t: "60%" } },
      { k: "row", id: "src", name: "Player", value: { shape: "choice", v: "spotify" } },
      { k: "row", id: "shuf", name: "Shuffle", value: { shape: "toggle", v: false } },
      { k: "row", id: "rep", name: "Repeat", value: { shape: "choice", v: "none" } }
    ]
  };
}

function dashScreen() {
  const out = ["Compiling serde v1.0.210", "Compiling mlua v0.9.9", "Compiling alacritty_terminal v0.24.1",
    "Compiling ratatui v0.29.0", "warning: unused variable: `rest`", "Compiling ranma v0.9.0 (~/src/ranma)"];
  const run = function (name, val, role) { return { k: "row", id: name, name: name, select: false, value: { t: val, role: role } }; };
  return {
    title: "build", chip: "BUILD", options: true, status: { t: "running", role: "accent" },
    subtitle: "cargo build --release", count: "pid 48211",
    keys: [["c", "cancel"], ["r", "restart"], ["l", "full log"]],
    card: [["c", "cancel the build"], ["r", "restart it"], ["l", "open the log in a pane"]],
    body: [
      { k: "heading", text: "Job" },
      { k: "facts", items: [["state", "running", { role: "accent" }], ["elapsed", "1:24", { strong: true }], ["jobs", "8"]] },
      { k: "progress", label: "crates", frac: 212 / 318, num: "212/318" },
      { k: "heading", text: "Output", count: "6 of 412" },
      { k: "log", lines: out.map(function (l, i) { return i === 4 ? [[l, "strong"]] : l; }) },
      { k: "heading", text: "Last runs", count: 3 },
      run("release · 2h ago", "✓ 2:31", "dim"),
      run("release · yesterday", "✗ 0:48", "urgent"),
      run("debug · yesterday", "✓ 0:39", "dim")
    ]
  };
}

// The settings panel scoped to one plugin's group: what `o` opens.
function optionsScreen() {
  return {
    title: "settings", chip: "SET", filter: "names", count: "4 of 61 options", detail: 3, sel: "notify",
    bottom: [["w", "save"], ["esc", "back to agents"]], keys: [["r", "default"]],
    body: [
      { k: "heading", text: "agents", tag: "plugin", count: 4 },
      { k: "row", id: "notify", name: "Notify when waiting", value: { shape: "toggle", v: true },
        detail: [{ k: "text", t: "A toast and an urgent badge when an agent stops to ask something.", max: 2 }, { k: "facts", items: [["default", "on", { strong: true }]] }] },
      { k: "row", id: "badge", name: "Badge", value: { shape: "choice", v: "glyph" } },
      { k: "row", id: "idle", name: "Idle after", mark: { t: "•", role: "normal" }, value: { shape: "slider", frac: 0.25, t: "30s" } },
      { k: "row", id: "cmds", name: "Agents", value: { shape: "field", v: "claude codex aider gemini" } }
    ]
  };
}

/* ---------- scenes ---------- */

function agentPanes(W, H) {
  const wA = W >= 160 ? 110 : Math.round(W / 2), hB = Math.floor(H / 2);
  return [
    { x: 0, y: 0, w: wA, h: H, idx: 1, name: "api", badges: [{ g: "?", t: "waiting", role: "urgent" }], draw: API },
    { x: wA, y: 0, w: W - wA, h: hB, idx: 2, name: "nvim", focus: true, draw: nvim },
    { x: wA, y: hB, w: W - wA, h: H - hB, idx: 3, name: "web", badges: [{ g: "●", t: "working", role: "accent" }], draw: WEB }
  ];
}
function plainPanes(W, H, first) {
  const wA = Math.round(W / 2), hB = Math.floor(H / 2);
  return [
    first || { x: 0, y: 0, w: wA, h: H, idx: 1, name: "nvim", focus: true, draw: nvim },
    { x: wA, y: 0, w: W - wA, h: hB, idx: 2, name: "zsh", draw: ZSH },
    { x: wA, y: hB, w: W - wA, h: H - hB, idx: 3, name: "cargo", draw: CARGO_OK }
  ];
}
function panelW(W) { return W >= 160 ? 96 : W >= 100 ? 56 : W >= 64 ? 44 : W; }

// A plugin screen: the panes laid out over the whole width, the screen floated
// over their right side (no resize), the bar's chip naming it.
function screenScene(W, H, bs, scr, panes, o) {
  o = o || {};
  const s = new Scr(W, H);
  const pw = panelW(W);
  if (pw < W) drawPanes(s, panes, bs);
  drawScreen(s, { x: W - pw, y: 0, w: pw, h: H - 1, bs: bs, scr: scr, help: o.help });
  drawBar(s, H - 1, W, scr.chip, { narrow: W < 64 });
  return s;
}

function peekScene(bs) {
  const W = 80, H = 24, S = agentsScreen();
  const s = new Scr(W, H);
  drawPanes(s, agentPanes(W, H - 1), bs);
  const row = S.body.filter(function (b) { return b.id === S.sel; })[0];
  drawPeek(s, bs, S, row, W, H);
  drawBar(s, H - 1, W, S.chip);
  return s;
}

function readme() {
  return segLines([
    [T_("")],
    [T_("  ranma", "magenta", true)],
    [T_("")],
    [T_("  A tiling window manager that runs inside a")],
    [T_("  terminal: panes, workspaces, a bar on one row.")],
    [T_("")],
    [T_("  It is extended by plugins written in Lua. See")],
    [T_("  the "), T_("plugin guide", "blue", false, "u"), T_(" for the events they hear")],
    [T_("  and the primitives they can draw with.")],
    [T_("")],
    [T_("  Install", "magenta", true)],
    [T_("")],
    [T_("    ./install.sh", "green")],
    [T_("")],
    [T_("  It checks the new binary against your config")],
    [T_("  and restores the previous one if it is refused.")]
  ], true);
}
function tooltipScene(bs) {
  const W = 80, H = 24, A = { x: 0, y: 0, w: W, h: H - 1 };
  const s = new Scr(W, H);
  let anchor, tip;
  {
    drawPanes(s, [
      { x: 0, y: 0, w: 56, h: 23, idx: 1, name: "glow README.md", focus: true, draw: readme() },
      { x: 56, y: 0, w: 24, h: 23, idx: 2, name: "zsh", draw: ZSH }
    ], bs);
    anchor = { x: 1 + 6 + 3, y: 1 + 7 };
    tip = { lines: [
      [[truncMid("https://github.com/masshirodev/ranma/blob/main/doc/CONFIG.md#plugins", 44), { fg: "toast_fg" }]],
      keyLine([["ctrl+click", "open"], ["ctrl+b y", "copy"]])
    ] };
  }
  s.rev(anchor.x, anchor.y);
  drawTooltip(s, anchor, tip, A, bs);
  drawBar(s, H - 1, W, null);
  return s;
}
function edgeTooltipScene(bs) {
  // the link at the pane's last rows, against the right edge: above, and pushed left
  const W = 80, H = 24, A = { x: 0, y: 0, w: W, h: H - 1 };
  const s = new Scr(W, H);
  const lines = [];
  for (let i = 0; i < 10; i++) lines.push([T_("")]);
  [
    [T_("$ ", "gray"), T_("cargo build")],
    [T_("   Compiling ranma")],
    [T_("error[E0308]:", "red", true)],
    [T_("mismatched types", "fg", true)],
    [T_("   |")],
    [T_("88 |  rest - 1")],
    [T_("   |  ^^^^^^^^ u16", "red")],
    [T_("")],
    [T_("error: could not", "red")],
    [T_("compile; see")],
    [T_("  --> "), T_("src/layout.rs:88", "blue", false, "u")]
  ].forEach(function (l) { lines.push(l); });
  drawPanes(s, [
    { x: 0, y: 0, w: 56, h: 23, idx: 1, name: "nvim", draw: nvim },
    { x: 56, y: 0, w: 24, h: 23, idx: 2, name: "cargo", focus: true, draw: segLines(lines) }
  ], bs);
  const anchor = { x: 57 + 6 + 12, y: 21 };
  s.rev(anchor.x, anchor.y);
  drawTooltip(s, anchor, { title: "src/layout.rs, line 88", lines: [
    [["opens in ", { fg: "bar_dim" }], ["1 nvim", { fg: "toast_fg" }], [" at line 88", { fg: "bar_dim" }]],
    keyLine([["ctrl+click", "open"], ["ctrl+b y", "copy"]])
  ] }, A, bs);
  drawBar(s, H - 1, W, null);
  return s;
}

function badgesScene(bs) {
  const W = 80, H = 24;
  const s = new Scr(W, H);
  const A = AGENT, cA = 22, cB = 38, cC = 20, r1 = 12, r2 = 11;
  drawPanes(s, [
    { x: 0, y: 0, w: cA, h: r1, idx: 1, name: "nvim", focus: true, draw: nvim },
    { x: cA, y: 0, w: cB, h: r1, idx: 2, name: "api", badges: [{ g: "?", t: "waiting", role: "urgent" }],
      draw: A([["● The client gives up on the first"], ["  503. I'll retry with backoff."], [""], ["Do you want to proceed?", "yellow", true], ["❯ 1. Yes", "fg"], ["  2. No, tell it what to do"]]) },
    { x: cA + cB, y: 0, w: cC, h: r1, idx: 3, name: "web", badges: [{ g: "●", t: "working", role: "accent" }],
      draw: A([["• Running npm test"], ["  PASS form (12)", "green"], ["  RUNS api", "yellow"]]) },
    { x: 0, y: r1, w: cA, h: r2, idx: 4, name: "docs", badges: [{ g: "✓", t: "done", role: "dim" }],
      draw: A([["Applied edit to", "gray"], ["doc/CONFIG.md", "gray"], ["> ", "gray"]]) },
    { x: cA, y: r1, w: cB, h: r2, idx: 5, name: "ranma", sync: true,
      badges: [{ g: "●", t: "working", role: "accent" }, { g: "✗", t: "build", role: "urgent" }],
      draw: A([["● Running cargo build"], ["  ⎿ error[E0308]: mismatched types", "red"], ["● Reading src/layout.rs"]]) },
    { x: cA + cB, y: r1, w: cC, h: r2, idx: 6, name: "infra",
      badges: [{ g: "?", t: "waiting", role: "urgent" }, { g: "✓", t: "build", role: "dim" }],
      draw: A([["Apply this plan?", "yellow", true], ["❯ Yes"], ["  No"]]) }
  ], bs);
  drawBar(s, H - 1, W, null);
  return s;
}

function ladderScene(bs) {
  const widths = [44, 36, 26, 20, 14, 9, 6];
  const W = 64, H = 2 + widths.length * 4;
  const s = new Scr(W, H);
  const p0 = { idx: 5, name: "api-gateway", sync: true, badges: [{ g: "●", t: "working", role: "accent" }, { g: "✗", t: "build", role: "urgent" }] };
  widths.forEach(function (w, i) {
    const y = 1 + i * 4;
    const run = titleRun(p0, w - 3, BS[bs].h, "border_inactive");
    s.put(2, y, w + " cells · " + LADDER[run.step], { fg: "bar_dim" });
    drawPanes(s, [Object.assign({}, p0, { x: 2, y: y + 1, w: w, h: 3 })], bs);
  });
  return s;
}

// Every block at the widths a list really has, like the settings panel's chart.
function blocksScene(bs) {
  const widths = [[69, "the 200×50 screen"], [40, "the 80×24 screen"], [36, "a nested ranma at 40×15"], [24, "narrower than ranma draws, for the record"]];
  const ctx = { hch: bs === "ascii" ? "-" : "─" };
  const blocks = [
    ["heading", { k: "heading", text: "Working", count: 2 }],
    ["row, text", { k: "row", id: "a", name: "web · codex", note: "1:code  ~/src/web", value: { t: "● 4m", role: "accent" } }, true],
    ["row, slider", { k: "row", id: "b", name: "Volume", value: { shape: "slider", frac: 0.6, t: "60%" } }],
    ["row, field", { k: "row", id: "c", name: "Title format", mark: { t: "•", role: "normal" }, value: { shape: "field", v: " {index}[ {program}] " } }],
    ["text", { k: "text", t: "Pick an agent to jump to its pane; a answers it without leaving the list." }],
    ["facts", { k: "facts", items: [["ws", "1:code"], ["pane", "1"], ["", "~/src/api"], ["waiting", "12m", { strong: true }]] }],
    ["progress", { k: "progress", label: "1:42", frac: 0.21, num: "8:09" }],
    ["log", { k: "log", lines: ["Compiling alacritty_terminal v0.24.1", "Compiling ranma v0.9.0 (/home/me/src/ranma)"] }],
    ["separator", { k: "sep" }]
  ];
  const per = function (cw) { return blocks.reduce(function (a, b) { return a + layoutBlocks([b[1]], cw, 2, ctx).length; }, 0); };
  const W = 16 + 69 + 6, H = widths.reduce(function (a, wd) { return a + per(wd[0]) + 4; }, 1);
  const s = new Scr(W, H);
  let top = 1;
  widths.forEach(function (wd) {
    const cw = wd[0];
    let y = top;
    top += per(cw) + 4;
    s.put(2, y, cw + " cells · " + wd[1], { fg: "bar_dim" });
    y += 1;
    const bx = 16;
    s.fill(bx, y, cw + 4, per(cw) + 2, { bg: "toast_bg", fg: "toast_fg" });
    y += 1;
    blocks.forEach(function (b) {
      const its = layoutBlocks([b[1]], cw, 2, ctx);
      s.put(2, y, b[0], { fg: "bar_dim" });
      its.forEach(function (it) { it.draw(s, bx + 2, y, !!b[2]); y++; });
    });
  });
  return s;
}

const SCENES = [
  { id: "agents80", title: "agents · 80×24", W: 80, H: 24 },
  { id: "agents80keys", title: "agents · 80×24 · ? keys", W: 80, H: 24 },
  { id: "agents80peek", title: "agents · 80×24 · space: peek", W: 80, H: 24 },
  { id: "agents200", title: "agents · 200×50", W: 200, H: 50 },
  { id: "agents40", title: "agents · nested ranma, 40×15", W: 40, H: 15 },
  { id: "history80", title: "history · 80×24", W: 80, H: 24 },
  { id: "media80", title: "media player · 80×24", W: 80, H: 24 },
  { id: "dash80", title: "dashboard · 80×24 · nothing to select", W: 80, H: 24 },
  { id: "options80", title: "agents → o: its options in settings · 80×24", W: 80, H: 24 },
  { id: "tipmid", title: "tooltip over a link, mid-pane · 80×24", W: 80, H: 24 },
  { id: "tipedge", title: "tooltip at the bottom-right edge: above, pushed left · 80×24", W: 80, H: 24 },
  { id: "badges80", title: "badges · 80×24", W: 80, H: 24 },
  { id: "ladder", title: "badges as a pane narrows", W: 64, H: 30 },
  { id: "blocks", title: "what shrinks: each block by list width", W: 91, H: 64 }
];

function buildScene(id, bs) {
  bs = BS[bs] ? bs : "rounded";
  if (id === "agents80") return screenScene(80, 24, bs, agentsScreen(), agentPanes(80, 23));
  if (id === "agents80keys") return screenScene(80, 24, bs, agentsScreen(), agentPanes(80, 23), { help: true });
  if (id === "agents80peek") return peekScene(bs);
  if (id === "agents200") return screenScene(200, 50, bs, agentsScreen(true), agentPanes(200, 49));
  if (id === "agents40") return screenScene(40, 15, bs, agentsScreen(), []);
  if (id === "history80") {
    const W = 80, H = 24, wA = 44;
    return screenScene(W, H, bs, historyScreen(), [
      { x: 0, y: 0, w: wA, h: 23, idx: 3, name: "cargo", draw: CARGO_PANIC },
      { x: wA, y: 0, w: W - wA, h: 23, idx: 1, name: "nvim", focus: true, draw: nvim }
    ]);
  }
  if (id === "media80") return screenScene(80, 24, bs, mediaScreen(), plainPanes(80, 23));
  if (id === "dash80") return screenScene(80, 24, bs, dashScreen(), plainPanes(80, 23));
  if (id === "options80") return screenScene(80, 24, bs, optionsScreen(), agentPanes(80, 23));
  if (id === "tipmid") return tooltipScene(bs);
  if (id === "tipedge") return edgeTooltipScene(bs);
  if (id === "badges80") return badgesScene(bs);
  if (id === "ladder") return ladderScene(bs);
  if (id === "blocks") return blocksScene(bs);
  return screenScene(80, 24, bs, agentsScreen(), agentPanes(80, 23));
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { buildScene: buildScene, toText: toText, toLines: toLines, SCENES: SCENES, THEMES: THEMES, BS: BS, CELLW: CELLW, ROWH: ROWH };
}

// node PLUGIN_PANEL_screen.js > PLUGIN_PANEL_MOCK.txt
if (typeof require !== "undefined" && typeof module !== "undefined" && require.main === module) {
  const out = ["# Plugin screens, tooltips and badges, cell for cell, as this handoff draws them",
    "# (PLUGIN_PANEL_screen.js, default theme; colour roles are in the script, not here).",
    "# ranma's tests compare against these. Regenerate only from a new handoff."];
  const styles = ["rounded", "none"];
  styles.forEach(function (bs) {
    SCENES.forEach(function (sc) {
      if (bs !== "rounded" && !(sc.W === 80 && /^(agents|history|media|dash|options)/.test(sc.id))) return;
      out.push("## " + sc.title + " · " + bs);
      out.push(toText(buildScene(sc.id, bs)).split("\n").map(function (l) { return l.replace(/ +$/, ""); }).join("\n"));
    });
  });
  console.log(out.join("\n"));
}
