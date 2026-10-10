/* lazy.ranma: the plugin manager's screen (card c196).
   Built on PLUGIN_PANEL_screen.js (c138): the same cells, frame, blocks and
   themes, with the primitives this screen adds to ranma's core marked NEW.
   The drawing is the design: where LAZY_PANEL.md and this script disagree,
   the script wins. `node LAZY_screen.js > LAZY_MOCK.txt` prints every scene. */

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
  } else if (sh === "meter") {
    // NEW: a read-only slider inside a row: what a task is doing, then how far.
    // No arrows (nothing to step). frac nil: no bar, the word and a time.
    const mw = v.frac == null ? 0 : CW >= 60 ? 13 : CW >= 40 ? 7 : CW >= 34 ? 5 : 0;
    P(v.t, sel ? { fg: "picker_selected_fg", b: true } : { fg: "bar_accent" });
    if (v.frac == null && v.time) P(" " + v.time, base);
    if (mw) {
      P(" ", base);
      const k = Math.round(v.frac * (mw - 1));
      for (let i = 0; i < mw; i++) {
        if (i < k) P("━", sel ? { fg: "picker_selected_fg" } : { fg: "bar_accent" });
        else if (i === k) P("●", sel ? { fg: "picker_selected_fg", b: true } : { fg: "toast_fg", b: true });
        else P("─", dim);
      }
      if (CW >= 60) P(" " + String(Math.round(v.frac * 100)).padStart(3) + "%", base);
    }
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
function putKeys(s, x, y, keys, maxX, noHelp) {
  if (noHelp) {
    // An ask's answers: all of them, no "? keys" (as settings' unsaved question).
    keys.forEach(function (kv, i) {
      if (i) x += 2;
      x = s.put(x, y, kv[0], { fg: kv[2] || "bar_accent", b: true });
      x = s.put(x + 1, y, kv[1], { fg: "bar_dim" });
    });
    return x;
  }
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
  // NEW (ask): the question takes the detail area, at its fixed height; on a
  // screen too short for a detail it takes all but 3 rows of the list.
  if (S.ask) {
    const want = layoutBlocks(S.ask.blocks, CW, 0, { hch: hch }).length;
    if (!D) D = Math.min(want, Math.max(1, body - 4));
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
  if (S.input) {
    // NEW (ask with text): the first row is the line being typed, in the filter's place.
    let qx = s.put(x0, qY, S.input.prefix, { fg: "bar_accent", b: true });
    qx = s.put(qx + 1, qY, S.input.text, { fg: "toast_fg", b: true });
    s.set(qx, qY, " ", { bg: "toast_fg" });
  } else if (S.filter) {
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
  if (D) fitDetail(S.ask ? S.ask.blocks : selRow.detail, D, CW, ctx).forEach(function (it, i) { it.draw(s, x0, rule1 + 1 + i, false); });

  if (S.ask) putKeys(s, x0, footY, S.ask.keys, x0 + CW, true);
  else putKeys(s, x0, footY, footKeys(S, selRow, wide, heads.length >= 2), x0 + CW);

  if (P.help) {
    const ch = 3 + (S.card || []).length + ranmaKeys(S).length;
    drawKeysCard(s, x + Math.floor((w - 40) / 2), ch > h - 4 ? y + 1 : y + 2, S, bs);
  }
  return selRow;
}

/* ---------- the keys card ---------- */

function ranmaKeys(S) {
  if (S.cardRanma) return S.cardRanma;
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
  // A module of the plugin's own, placed first on the right side: pieces of [text, role].
  if (opt.mods && opt.mods.length) {
    const tw = opt.mods.reduce(function (a, m, i) { return a + (i ? 1 : 0) + len(m[0]); }, 0);
    let mx = W - len(r1) - len(r2) - tw - 2;
    opt.mods.forEach(function (m, i) {
      if (i) mx++;
      mx = s.put(mx, y, m[0], roleSt(m[1], false));
    });
  }
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

/* ---------- what is in the panes behind the screen ---------- */

const LUA_KW = /^(local|function|end|return|require|if|then|else|for|in|do|true|false|nil|and|or|not)$/;
function luaLine(src) {
  if (/^\s*--/.test(src)) return [[src, "gray"]];
  const segs = [];
  const re = /(\s+|[A-Za-z_][A-Za-z0-9_]*|\d+|"[^"]*"|.)/g;
  let m;
  while ((m = re.exec(src))) {
    const t = m[0];
    let r = "fg";
    if (/^\s+$/.test(t)) r = "fg";
    else if (LUA_KW.test(t)) r = "magenta";
    else if (/^\d/.test(t)) r = "orange";
    else if (t[0] === "\"") r = "green";
    else if (t === "ranma" || t === "opts") r = "yellow";
    else if (/^[{}()\[\];:,.<>=+\-]$/.test(t)) r = "gray";
    segs.push([t, r]);
  }
  return segs;
}
function nvimLua(file, lines, cur) {
  return function (s, x, y, w, h) {
    for (let i = 0; i < h - 1 && i < lines.length; i++) {
      let cx = s.put(x, y + i, String(i + 1).padStart(3) + " ", { fg: i === cur ? "yellow" : "gray" });
      luaLine(lines[i]).forEach(function (sg) { cx = s.put(cx, y + i, sg[0], { fg: sg[1] }); });
    }
    for (let i = lines.length; i < h - 1; i++) s.put(x, y + i, "~", { fg: "blue" });
    s.fill(x, y + h - 1, w, 1, { bg: "surface" });
    const cx = s.put(x, y + h - 1, " NORMAL ", { fg: "bg", bg: "blue", b: true });
    s.put(cx, y + h - 1, " " + file + " ", { fg: "fg", bg: "surface" });
  };
}
const INIT_LUA = [
  "-- ~/.config/ranma/init.lua",
  "require(\"lazy\").setup {",
  "  \"masshirodev/history.ranma\",",
  "  { dir = \"~/projects/agents.ranma\" },",
  "  \"masshirodev/notes.ranma\",",
  "  { import = \"plugins\" },",
  "}",
  "",
  "ranma.set { border = \"rounded\" }",
  "ranma.bind(\"leader L\", function()",
  "  require(\"lazy\").open()",
  "end, { desc = \"plugins\" })",
  "",
  "ranma.bar {",
  "  left = { \"mode\", \"workspaces\" },",
  "  right = { \"lazy\", \"cpu\", \"mem\", \"clock\" },",
  "}"
];
const FIRST_LUA = [
  "-- ~/.config/ranma/init.lua",
  "local dir = ranma.data .. \"/lazy/lazy.ranma\"",
  "if not ranma.exists(dir) then",
  "  ranma.git_clone(\"masshirodev/lazy.ranma\", dir)",
  "end",
  "ranma.rtp_prepend(dir)",
  "",
  "require(\"lazy\").setup {",
  "  { import = \"plugins\" },",
  "}"
];
const COMPOSE_LUA = [
  "-- lua/plugins/compose.lua",
  "return {",
  "  \"masshirodev/compose.ranma\",",
  "  opts = { keep = 30, submit = false },",
  "}"
];
const ZSH_LAZY = segLines([
  [T_("~/.config/ranma", "cyan"), T_(" main", "magenta")],
  [T_("❯ ", "green"), T_("ls lua/plugins")],
  [T_("build.lua    clock.lua    compose.lua")],
  [T_("links.lua    marks.lua    media.lua")],
  [T_("zen.lua")],
  [T_("~/.config/ranma", "cyan"), T_(" main", "magenta")],
  [T_("❯ ", "green"), T_("█")]
]);
function lazyPanes(W, H, left) {
  const wA = W >= 160 ? 104 : Math.round(W / 2), hB = Math.floor(H / 2);
  return [
    { x: 0, y: 0, w: wA, h: H, idx: 1, name: "nvim", focus: true, draw: left || nvimLua("init.lua", INIT_LUA, 2) },
    { x: wA, y: 0, w: W - wA, h: hB, idx: 2, name: "zsh", draw: ZSH_LAZY },
    { x: wA, y: hB, w: W - wA, h: H - hB, idx: 3, name: "cargo", draw: CARGO_OK }
  ];
}

/* ---------- the plugins ----------
   Each is a spec (where it was declared, what it is checked out at) and a state.
   A row's value carries the state with a glyph, so it reads without colour:
     ● loaded (load time)   ↑ behind (commits)   ✗ failed (what failed)
     ○ not installed        – disabled / skipped  ◇ in no spec (size)
     a meter: working (clone, fetch, checkout, build)   ✓ done in this run
   A row's mark, after the name, is what the spec says about it:
     local · pinned · off lock (accent) · deletes (urgent, while an ask stands) */

const LAZY_DIR = "~/.local/share/ranma/lazy/";
const PL = {
  lazy:    { name: "lazy.ranma",    from: "init.lua:2", br: "main", at: "4be1c07", ms: "0.6ms" },
  history: { name: "history.ranma", from: "init.lua:3", br: "main", at: "a3f91c2", ms: "0.4ms", behind: 3 },
  agents:  { name: "agents.ranma",  from: "init.lua:4", dir: "~/projects/agents.ranma", br: "wip", at: "41aa0c7", ms: "1.4ms" },
  notes:   { name: "notes.ranma",   from: "init.lua:5" },
  compose: { name: "compose.ranma", from: "lua/plugins/compose.lua:1", br: "main", at: "c11a9e0", ms: "0.3ms", behind: 1 },
  media:   { name: "media.ranma",   from: "lua/plugins/media.lua:1", br: "main", at: "9b2d6f8" },
  build:   { name: "build.ranma",   from: "lua/plugins/build.lua:1", br: "main", at: "0de93b2" },
  marks:   { name: "marks.ranma",   from: "lua/plugins/marks.lua:1", br: "main", at: "1f3a9c0", ms: "0.5ms", behind: 12 },
  links:   { name: "links.ranma",   from: "lua/plugins/links.lua:1", tag: "v0.3.0", at: "e0c4f3a", ms: "0.3ms" },
  clock:   { name: "clock.ranma",   from: "lua/plugins/clock.lua:1", br: "main", at: "9c1e2aa", lock: "58be7d1", ms: "0.2ms" },
  zen:     { name: "zen.ranma",     from: "lua/plugins/zen.lua:1", br: "main", at: "7d02e4b" },
  ssh:     { name: "ssh-hosts",     from: "pack/github/start/ssh-hosts", at: "0de93b2", ms: "0.4ms" },
  scratch: { name: "scratch.ranma", size: "220K", seen: "3 weeks ago" },
  oldtheme:{ name: "old-theme.ranma", size: "48K", seen: "2 months ago" },
  panenote:{ name: "pane-notes.ranma", size: "812K", seen: "5 days ago" },
  sessions:{ name: "sessions.ranma", from: "lua/plugins/sessions.lua:1" }
};
const SUBJECTS = {
  history: [["7d02e4b", "keep the query when the pane scrolls"], ["c11a9e0", "match across wrapped lines"], ["58be7d1", "doc: the on_query delay"]],
  compose: [["e3b8f01", "submit = false keeps the draft"]],
  marks: [["2c9d4e1", "marks survive a reload"], ["8a71b3c", "jump list per workspace"], ["f04e2d9", "doc: marks and copy mode"]]
};

function shortFrom(p) { return p.dir ? p.dir : p.from ? p.from.replace(/^lua\//, "") : "no spec"; }
function commitFacts(p, extra) {
  const it = [];
  if (p.tag) it.push(["tag", p.tag]); else if (p.br) it.push(["branch", p.br]);
  if (p.at) it.push(["at", p.at]);
  (extra || []).forEach(function (e) { it.push(e); });
  if (p.ms) it.push(["load", p.ms]);
  return { k: "facts", items: it };
}
function fromFacts(p) { return { k: "facts", items: [["from", p.from]] }; }
function subjLog(id, n) {
  return { k: "log", lines: SUBJECTS[id].slice(0, n || 9).map(function (c) { return [[c[0] + " ", "num"], [c[1]]]; }) };
}

const VAL = {
  loaded:  function (p) { return { t: "● " + p.ms, role: "dim" }; },
  update:  function (p) { return { t: "↑ " + p.behind, role: "accent" }; },
  failload:function () { return { t: "✗ load", role: "urgent" }; },
  failtask:function (p, o) { return { t: "✗ " + o.task, role: "urgent" }; },
  missing: function () { return { t: "○ missing", role: "normal" }; },
  disabled:function () { return { t: "– off", role: "dim" }; },
  stray:   function (p) { return { t: "◇ " + p.size, role: "dim" }; },
  working: function (p, o) { return { shape: "meter", t: o.task, frac: o.frac, time: o.time }; },
  done:    function (p, o) { return { t: o.t, role: o.role || "normal" }; }
};
const KEYS = {
  loaded:  [["u/U", "update"], ["l", "log"], ["x", "remove"]],
  update:  [["u/U", "update"], ["l", "log"], ["x", "remove"]],
  failload:[["e", "edit"], ["u/U", "update"], ["l", "log"]],
  failtask:[["b", "rebuild"], ["u/U", "update"], ["x", "remove"]],
  missing: [["i/I", "install"], ["x", "remove"]],
  disabled:[["u/U", "update"], ["x", "remove"]],
  local:   [["enter", "open"], ["x", "remove"]],
  pinned:  [["r/R", "restore"], ["l", "log"], ["x", "remove"]],
  offlock: [["r/R", "restore"], ["u/U", "update"], ["l", "log"]],
  stray:   [["x/X", "delete"], ["enter", "open"]],
  hand:    [["enter", "open"], ["y", "copy URL"]],
  working: [["enter", "open"], ["l", "log"]],
  done:    [["l", "log"], ["enter", "open"]]
};

function detailFor(id, st, o) {
  const p = PL[id];
  o = o || {};
  if (o.detail) return o.detail;
  if (st === "update") return [subjLog(id), commitFacts(p, [["behind", String(p.behind), { role: "accent", strong: true }]]), fromFacts(p)];
  if (st === "failload") return [
    { k: "text", t: "lua/media/init.lua:42: attempt to index a nil value (field 'player')", role: "urgent", max: 2 },
    { k: "text", t: "e opens lua/media/init.lua:42 in a pane.", role: "dim", max: 1 },
    commitFacts(p, [["", "not loaded", { role: "urgent" }]]), fromFacts(p)];
  if (st === "failtask") return [
    { k: "log", lines: [[["$ make", "strong"]], "cc -O2 -o bin/watch src/watch.c", "make: cc: No such file or directory", "make: *** [Makefile:4: bin/watch] Error 127"] },
    { k: "facts", items: [["build", "make"], ["exit", "127", { role: "urgent", strong: true }], ["at", p.at]] }, fromFacts(p)];
  if (st === "missing") return [
    { k: "text", t: "Declared, not cloned yet: i installs it, and so does the next sync.", max: 2 },
    { k: "facts", items: [["", "github.com/masshirodev/" + p.name]] }, fromFacts(p)];
  if (st === "disabled") return [
    { k: "text", t: "enabled = false: installed, not loaded. Updates still reach it.", max: 2 },
    commitFacts(p), fromFacts(p)];
  if (st === "local") return [
    { k: "text", t: "Your checkout: loaded as it is, never fetched, reset or deleted.", max: 2 },
    { k: "facts", items: [["dir", p.dir]] }, commitFacts(p)];
  if (st === "pinned") return [
    { k: "text", t: "pin = true: update and sync skip it.", max: 2 },
    commitFacts(p), fromFacts(p)];
  if (st === "offlock") return [
    { k: "text", t: "At " + p.at + "; the lock says " + p.lock + ". r goes back to it; u updates and locks.", role: "accent", max: 2 },
    commitFacts(p), fromFacts(p)];
  if (st === "stray") return [
    { k: "text", t: "No spec declares it. x deletes this clone, X every stray; both ask first.", max: 2 },
    { k: "facts", items: [["dir", "lazy/" + p.name], ["size", p.size], ["loaded", p.seen]] }];
  if (st === "hand") return [
    { k: "text", t: "Cloned by hand into pack/*/start/: ranma loads it; the manager leaves it be.", max: 2 },
    commitFacts(p)];
  return [{ k: "text", t: "Up to date · checked 12m ago", role: "dim" }, commitFacts(p), fromFacts(p)];
}

// R(id, state, o): one row. state names the value, keys and detail; o overrides.
function R(id, st, o) {
  o = o || {};
  const p = PL[id];
  const base = st === "local" || st === "pinned" || st === "offlock" || st === "hand" ? "loaded" : st;
  const r = { k: "row", id: id, name: p.name, note: o.note || shortFrom(p), value: (VAL[base] || VAL.loaded)(p, o),
    keys: o.keys || KEYS[st] || KEYS.loaded, detail: detailFor(id, st, o) };
  if (o.value) r.value = o.value;
  const mk = o.mark || (p.dir ? "local" : p.tag ? "pinned" : st === "offlock" || (st === "loaded" && p.lock) ? "off lock" : null);
  if (mk) r.mark = { t: mk, role: mk === "off lock" ? "accent" : mk === "deletes" ? "urgent" : "dim" };
  return r;
}
function H(text, n) { return { k: "heading", text: text, count: n }; }

const SCREEN_KEYS = [["S", "sync"], ["a", "add"], ["C", "check"]];
const CARD = [
  ["u U", "update it · all"],
  ["i I", "install it · all missing"],
  ["r R", "restore it · all (the lock)"],
  ["x X", "remove it · clean (asks)"],
  ["C", "check all for updates"],
  ["S", "sync: install, clean, pull"],
  ["a", "add one: owner/repo"],
  ["l", "the log of its updates"],
  ["b", "run its build again", "build"],
  ["e", "edit at the error", "load"],
  ["enter", "open its folder in a pane"],
  ["y", "copy its URL"],
  ["o", "its options in settings"]
];
const CARD_RANMA = [["↑↓ j k", "move"], ["tab", "next group"], ["/", "filter by name"], ["space", "peek at the panes"], ["esc", "close"]];

function lazyScreen(o) {
  return Object.assign({
    title: "lazy", chip: "LAZY", options: true, filter: "names", count: "13 plugins", detail: 5,
    status: { t: "2 failed", role: "urgent" }, keys: SCREEN_KEYS, card: CARD, cardRanma: CARD_RANMA
  }, o);
}

// The usual list: every state, in sections, the sections in order of urgency.
function mainBody(over) {
  over = over || {};
  const r = function (id, st, o) { return over[id] ? over[id] : R(id, st, o); };
  const body = [
    H("Failed", 2), r("media", "failload"), r("build", "failtask", { task: "build" }),
    H("Updates", 3), r("history", "update"), r("compose", "update"), r("marks", "update"),
    H("Not installed", 1), r("notes", "missing"),
    H("Installed", 5), r("agents", "local"), r("clock", "offlock"), r("lazy", "loaded"), r("links", "pinned"), r("zen", "disabled"),
    H("Not in spec", 1), r("scratch", "stray"),
    H("Not managed", 1), r("ssh", "hand")
  ];
  return body;
}

/* ---------- the screens, scene by scene ---------- */

function listScreen() { return lazyScreen({ sel: "media", body: mainBody() }); }
function listWide() { return lazyScreen({ sel: "history", body: mainBody() }); }

function syncScreen() {
  const D = function (t, role) { return { value: { t: t, role: role || "normal" } }; };
  const mk = function (id, st, o) { return R(id, st, o); };
  const body = [
    H("Failed", 2),
    mk("media", "done", { t: "✓ +1", detail: [{ k: "text", t: "Pulled 1 commit. It loads again when the sync ends and ranma reloads.", max: 2 }, subjLogRaw([["5a0e7c2", "guard a missing player"]]), commitFacts(PL.media)] }),
    mk("build", "working", { task: "build", time: "0:04", detail: [
      { k: "log", lines: [[["$ make", "strong"]], "cc -O2 -c src/watch.c", "cc -O2 -c src/tail.c"] },
      { k: "facts", items: [["build", "make"], ["running", "0:04", { strong: true }], ["at", "6e2f1d0"]] }, fromFacts(PL.build)] }),
    H("Updates", 3),
    mk("history", "done", { t: "✓ +3", detail: [subjLog("history"), commitFacts({ br: "main", at: "7d02e4b" }, [["was", "a3f91c2"]]), fromFacts(PL.history)] }),
    mk("compose", "working", { task: "fetch", frac: 0.6, detail: [
      { k: "log", lines: ["remote: Enumerating objects: 9, done.", "Receiving objects:  60% (5/9)"] }, commitFacts(PL.compose), fromFacts(PL.compose)] }),
    mk("marks", "done", { t: "✗ pull", role: "urgent", keys: [["u", "retry"], ["enter", "open"], ["l", "log"]], detail: [
      { k: "log", lines: [[["$ git merge --ff-only origin/main", "strong"]], "error: Your local changes would be overwritten", "  by merge: lua/marks.lua", "Aborting"] },
      { k: "facts", items: [["pull", "exit 1", { role: "urgent", strong: true }], ["at", PL.marks.at], ["behind", "12"]] }, fromFacts(PL.marks)] }),
    H("Not installed", 1),
    mk("notes", "working", { task: "clone", frac: 0.3, detail: [
      { k: "log", lines: ["Cloning into 'notes.ranma'...", "Receiving objects:  30% (64/214)"] }, { k: "facts", items: [["", "github.com/masshirodev/notes.ranma"]] }, fromFacts(PL.notes)] }),
    H("Installed", 5),
    mk("agents", "done", { t: "– skipped", role: "dim" }),
    mk("clock", "done", { t: "✓ +2" }),
    mk("lazy", "done", { t: "✓ same", role: "dim" }),
    mk("links", "done", { t: "– skipped", role: "dim" }),
    mk("zen", "done", { t: "✓ same", role: "dim" }),
    H("Not in spec", 1),
    mk("scratch", "done", { t: "✓ deleted", role: "dim" }),
    H("Not managed", 1),
    R("ssh", "hand")
  ];
  return lazyScreen({ sel: "build", body: body, status: { t: "sync 7/10 · 1 failed", role: "urgent" } });
}
function subjLogRaw(cs) { return { k: "log", lines: cs.map(function (c) { return [[c[0] + " ", "num"], [c[1]]]; }) }; }

// S with something to clean: sync asks first, since it would delete.
function syncAskScreen() {
  const body = mainBody({ scratch: R("scratch", "stray", { mark: "deletes" }) });
  return lazyScreen({ sel: "scratch", body: body, status: { t: "sync?", role: "urgent" },
    ask: {
      blocks: [
        { k: "text", t: "Sync deletes 1 clone no spec declares:", role: "urgent", strong: true },
        { k: "log", lines: [[["scratch.ranma", "strong"], ["  220K · loaded 3 weeks ago"]]] },
        { k: "text", t: "Then it installs 1, updates 8 and skips 2 (local, pinned).", max: 2 },
        { k: "space" }
      ],
      keys: [["y", "sync", "bar_urgent"], ["n", "sync, keep it"], ["esc", "cancel"]]
    } });
}

function firstScreen() {
  const det = function (pct, n) {
    return [{ k: "log", lines: ["Cloning into '" + "x" + "'...", "Receiving objects:  " + pct + "% (" + n + ")"] }];
  };
  const clone = function (id, frac, n) {
    const p = PL[id];
    return R(id, "working", { task: "clone", frac: frac, detail: [
      { k: "log", lines: ["Cloning into '" + p.name + "'...", "remote: Enumerating objects: " + n + ", done.", "Receiving objects:  " + Math.round(frac * 100) + "% (" + Math.round(frac * n) + "/" + n + ")"] },
      { k: "facts", items: [["", "github.com/masshirodev/" + p.name]] }, fromFacts(p)] });
  };
  void det;
  return lazyScreen({
    filter: false, subtitle: "opened to install 3 plugins", count: "4 plugins",
    status: { t: "installing 0/3", role: "accent" }, sel: "compose",
    body: [
      H("Not installed", 3), clone("compose", 0.72, 96), clone("history", 0.35, 328), clone("media", 0.08, 1204),
      H("Installed", 1), R("lazy", "loaded", { detail: [{ k: "text", t: "The manager itself: it cloned itself from init.lua, and updates like the rest.", max: 2 }, commitFacts(PL.lazy), fromFacts(PL.lazy)] })
    ]
  });
}

function addScreen() {
  return lazyScreen({ sel: "media", body: mainBody(), count: "", input: { prefix: "+", text: "masshirodev/sessions.ranma" },
    status: { t: "add", role: "accent" },
    ask: {
      blocks: [
        { k: "text", t: "owner/repo, or a git URL.", strong: true },
        { k: "text", t: "Writes lua/plugins/sessions.lua with its spec; ranma reloads and installs it.", role: "dim", max: 2 },
        { k: "facts", items: [["writes", "lua/plugins/sessions.lua"]] },
        { k: "space" }
      ],
      keys: [["enter", "add"], ["esc", "cancel"], ["ctrl+u", "clear"]]
    } });
}

function addedScreen() {
  const p = PL.sessions;
  const row = R("sessions", "working", { task: "clone", frac: 0.45, mark: "new", detail: [
    { k: "log", lines: ["Cloning into 'sessions.ranma'...", "remote: Enumerating objects: 214, done.", "Receiving objects:  45% (97/214)"] },
    { k: "facts", items: [["", "github.com/masshirodev/sessions.ranma"]] }, fromFacts(p)] });
  row.mark = { t: "new", role: "accent" };
  const body = mainBody();
  const i = body.findIndex(function (b) { return b.text === "Not installed"; });
  body[i] = H("Not installed", 2);
  body.splice(i + 2, 0, row);
  return lazyScreen({ sel: "sessions", body: body, count: "14 plugins", status: { t: "installing 1/1", role: "accent" } });
}

function removeScreen() {
  const body = mainBody({ compose: R("compose", "update", { mark: "deletes" }) });
  return lazyScreen({ sel: "compose", body: body, status: { t: "remove?", role: "urgent" },
    ask: {
      blocks: [
        { k: "text", t: "Remove compose.ranma?", role: "urgent", strong: true },
        { k: "log", lines: [[["deletes ", "num"], ["lua/plugins/compose.lua"]], [["deletes ", "num"], ["its clone, lazy/compose.ranma"]]] },
        { k: "text", t: "ranma reloads without it.", role: "dim" }
      ],
      keys: [["y", "remove", "bar_urgent"], ["esc", "keep it"]]
    } });
}

function removeInlineScreen() {
  return lazyScreen({ sel: "history", body: mainBody(), status: { t: "remove", role: "normal" },
    ask: {
      blocks: [
        { k: "text", t: "history.ranma is declared in init.lua:3.", strong: true },
        { k: "text", t: "The manager never rewrites init.lua: delete that line, save, and X cleans the clone up.", max: 3 },
        { k: "space" }
      ],
      keys: [["e", "edit init.lua:3"], ["esc", "back"]]
    } });
}

function cleanScreen() {
  const body = mainBody();
  const i = body.findIndex(function (b) { return b.text === "Not in spec"; });
  body.splice(i, 2, H("Not in spec", 3),
    R("oldtheme", "stray", { mark: "deletes" }), R("panenote", "stray", { mark: "deletes" }), R("scratch", "stray", { mark: "deletes" }));
  return lazyScreen({ sel: "oldtheme", body: body, count: "15 plugins", status: { t: "clean?", role: "urgent" },
    ask: {
      blocks: [
        { k: "text", t: "Delete 3 clones no spec declares?", role: "urgent", strong: true },
        { k: "log", lines: [
          [["old-theme.ranma ", "strong"], ["   48K  2 months ago"]],
          [["pane-notes.ranma", "strong"], ["  812K  5 days ago"]],
          [["scratch.ranma   ", "strong"], ["  220K  3 weeks ago"]]] },
        { k: "facts", items: [["", "1.1M in all"]] }
      ],
      keys: [["y", "delete 3", "bar_urgent"], ["esc", "keep them"]]
    } });
}

function logScreen() {
  const c = function (id, name, age, hash, stat) {
    return { k: "row", id: id, name: name, note: hash, value: { t: age, role: "dim" },
      keys: [["enter", "show"], ["y", "copy hash"]],
      detail: [{ k: "log", lines: stat }, { k: "facts", items: [["", hash], ["by", "masshiro"], ["", age + " ago"]] }] };
  };
  return lazyScreen({
    filter: false, subtitle: "log · the last 7 days", count: "4 plugins", detail: 5, status: null, sel: "h1",
    keys: [], options: false, bottom: [["esc", "back"]],
    body: [
      { k: "heading", text: "history.ranma", tag: "a3f91c2 → 7d02e4b", count: 3 },
      c("h1", "keep the query when the pane scrolls", "2h", "7d02e4b", ["lua/history/screen.lua   +31 −8", "lua/history/query.lua     +4 −2", "2 files changed"]),
      c("h2", "match across wrapped lines", "2h", "c11a9e0", ["lua/history/search.lua  +58 −12"]),
      c("h3", "doc: the on_query delay", "2h", "58be7d1", ["README.md  +6 −1"]),
      { k: "heading", text: "clock.ranma", tag: "58be7d1 → 9c1e2aa", count: 2 },
      c("c1", "show seconds while the bar is large", "1d", "9c1e2aa", ["lua/clock.lua  +12 −3"]),
      c("c2", "format option takes os.date's", "1d", "3d4a6b0", ["lua/clock.lua  +5 −5"]),
      { k: "heading", text: "media.ranma", tag: "41aa0c7 → 9b2d6f8", count: 1 },
      c("m1", "players from playerctl -l", "4d", "9b2d6f8", ["lua/media/init.lua  +22 −9"]),
      { k: "heading", text: "lazy.ranma", tag: "e3b8f01 → 4be1c07", count: 1 },
      c("l1", "lock where each update lands", "6d", "4be1c07", ["lua/lazy/lock.lua  +40 −11"])
    ]
  });
}

/* ---------- the update notice, outside the screen ---------- */

// A toast: a box at the top right that never takes keys (DESIGN.md, "Toasts").
function drawToast(s, x, y, w, title, lines, bs, role) {
  const h = lines.length + 2;
  const bst = { fg: role === "urgent" ? "bar_urgent" : "bar_accent", bg: bs === "none" ? "toast_bg" : "bg" };
  s.fill(x, y, w, h, { bg: "toast_bg", fg: "toast_fg" });
  s.box(x, y, w, h, bs, bst);
  s.put(x + 2, y, " " + title + " ", { fg: "toast_fg", bg: bst.bg, b: true });
  lines.forEach(function (l, i) {
    let cx = x + 2;
    l.forEach(function (sg) { cx = s.put(cx, y + 1 + i, sg[0], sg[1]); });
  });
}
const MOD = {
  updates: [["↑ 3 updates", "accent"]],
  updatesN: [["↑3", "accent"]],
  failed: [["✗ 1 failed", "urgent"], ["↑ 3", "accent"]],
  failedN: [["✗1", "urgent"], ["↑3", "accent"]],
  sync: [["● sync 7/10", "accent"]],
  syncN: [["● 7/10", "accent"]]
};
function noticeScene(bs) {
  const W = 80, H = 24;
  const s = new Scr(W, H);
  drawPanes(s, lazyPanes(W, H - 1), bs);
  drawToast(s, W - 40, 1, 39, "lazy", [
    [["3 plugins have updates: ", { fg: "toast_fg" }], ["history,", { fg: "toast_fg", b: true }]],
    [["compose", { fg: "toast_fg", b: true }], [" and ", { fg: "toast_fg" }], ["marks", { fg: "toast_fg", b: true }], [".", { fg: "toast_fg" }]],
    [["leader L", { fg: "bar_accent", b: true }], [" shows them; checked 16:30", { fg: "bar_dim" }]]
  ], bs, "accent");
  drawBar(s, H - 1, W, null, { mods: MOD.updates });
  return s;
}
function barModScene() {
  const rows = [
    ["nothing to say: the module renders nil and takes no room", null, false],
    ["updates found by the timer's check", MOD.updates, false],
    ["a plugin failed to load: urgent first", MOD.failed, false],
    ["a sync running, screen closed", MOD.sync, false],
    ["the same four on a 40-column ranma", null, true],
    ["", MOD.updatesN, true],
    ["", MOD.failedN, true],
    ["", MOD.syncN, true]
  ];
  const W = 80, H = rows.length * 3 + 1;
  const s = new Scr(W, H);
  rows.forEach(function (r, i) {
    const y = 1 + i * 3;
    if (r[0]) s.put(2, y, r[0], { fg: "bar_dim" });
    const sub = new Scr(r[2] ? 40 : W, 1);
    drawBar(sub, 0, r[2] ? 40 : W, null, { mods: r[1], narrow: r[2] });
    for (let x = 0; x < sub.W; x++) {
      const c = sub.cells[x];
      s.cells[(y + 1) * W + x] = Object.assign({}, c);
    }
  });
  return s;
}

/* ---------- peek ---------- */
function lazyPeek(bs) {
  const W = 80, H = 24, S = listWide();
  const s = new Scr(W, H);
  drawPanes(s, lazyPanes(W, H - 1), bs);
  const row = S.body.filter(function (b) { return b.id === S.sel; })[0];
  drawPeek(s, bs, S, row, W, H);
  drawBar(s, H - 1, W, S.chip);
  return s;
}

/* ---------- the detail, one state at a time ---------- */
function detailsScene(bs) {
  const CWd = 40;
  const ctx = { hch: bs === "ascii" ? "-" : "─" };
  const states = [
    ["loaded", R("lazy", "loaded")],
    ["update available", R("history", "update")],
    ["failed to load", R("media", "failload")],
    ["failed task", R("build", "failtask", { task: "build" })],
    ["working", syncScreen().body.filter(function (b) { return b.id === "compose"; })[0]],
    ["not installed", R("notes", "missing")],
    ["disabled", R("zen", "disabled")],
    ["pinned", R("links", "pinned")],
    ["local", R("agents", "local")],
    ["off the lock", R("clock", "offlock")],
    ["not in the spec", R("scratch", "stray")],
    ["not managed", R("ssh", "hand")]
  ];
  const D = 5;
  const per = 1 + 1 + D + 3;
  const cols = 2;
  const colW = 18 + CWd + 6;
  const W = colW * cols + 2, Hh = Math.ceil(states.length / cols) * per + 1;
  const s = new Scr(W, Hh);
  states.forEach(function (st, i) {
    const bx = 2 + (i % cols) * colW, top = 1 + Math.floor(i / cols) * per;
    s.put(bx, top + 1, st[0], { fg: "bar_dim" });
    const fx = bx + 16;
    s.fill(fx, top, CWd + 4, D + 4, { bg: "toast_bg", fg: "toast_fg" });
    drawRow(s, fx + 2, top + 1, CWd, st[1], true, {});
    for (let x = fx; x < fx + CWd + 4; x++) s.set(x, top + 2, ctx.hch, { fg: "border_inactive", bg: "toast_bg" });
    fitDetail(st[1].detail, D, CWd, ctx).forEach(function (it, j) { it.draw(s, fx + 2, top + 3 + j, false); });
  });
  return s;
}

/* ---------- scenes ---------- */

const SCENES = [
  { id: "list80", title: "the list · 80×24", W: 80, H: 24 },
  { id: "list80keys", title: "? keys · 80×24", W: 80, H: 24 },
  { id: "list200", title: "the list · 200×50", W: 200, H: 50 },
  { id: "list40", title: "nested ranma · 40×15", W: 40, H: 15 },
  { id: "details", title: "the detail, by state · 40 cells", W: 130, H: 61 },
  { id: "syncask80", title: "S: sync asks before it deletes · 80×24", W: 80, H: 24 },
  { id: "sync80", title: "a sync in progress · 80×24", W: 80, H: 24 },
  { id: "first80", title: "first run: three plugins cloning · 80×24", W: 80, H: 24 },
  { id: "add80", title: "a: typing owner/repo · 80×24", W: 80, H: 24 },
  { id: "added80", title: "a: the new plugin installing · 80×24", W: 80, H: 24 },
  { id: "remove80", title: "x: a plugin with its own file · 80×24", W: 80, H: 24 },
  { id: "removeinline80", title: "x: a plugin declared in init.lua · 80×24", W: 80, H: 24 },
  { id: "clean80", title: "X: cleaning three strays · 80×24", W: 80, H: 24 },
  { id: "log80", title: "l: what updates brought · 80×24", W: 80, H: 24 },
  { id: "peek80", title: "space: peek · 80×24", W: 80, H: 24 },
  { id: "notice80", title: "screen closed: the toast and the bar module · 80×24", W: 80, H: 24 },
  { id: "barmod", title: "the bar module, by state and width", W: 80, H: 25 }
];

function buildScene(id, bs) {
  bs = BS[bs] ? bs : "rounded";
  const P = function (left) { return lazyPanes(80, 23, left); };
  if (id === "list80") return screenScene(80, 24, bs, listScreen(), P());
  if (id === "list80keys") return screenScene(80, 24, bs, listScreen(), P(), { help: true });
  if (id === "list200") return screenScene(200, 50, bs, listWide(), lazyPanes(200, 49));
  if (id === "list40") return screenScene(40, 15, bs, listScreen(), []);
  if (id === "details") return detailsScene(bs);
  if (id === "syncask80") return screenScene(80, 24, bs, syncAskScreen(), P());
  if (id === "sync80") return screenScene(80, 24, bs, syncScreen(), P());
  if (id === "first80") return screenScene(80, 24, bs, firstScreen(), P(nvimLua("init.lua", FIRST_LUA, 3)));
  if (id === "add80") return screenScene(80, 24, bs, addScreen(), P());
  if (id === "added80") return screenScene(80, 24, bs, addedScreen(), P(nvimLua("sessions.lua", ["-- lua/plugins/sessions.lua", "-- written by lazy.ranma; yours to edit", "return {", "  \"masshirodev/sessions.ranma\",", "}"], 3)));
  if (id === "remove80") return screenScene(80, 24, bs, removeScreen(), P(nvimLua("compose.lua", COMPOSE_LUA, 2)));
  if (id === "removeinline80") return screenScene(80, 24, bs, removeInlineScreen(), P());
  if (id === "clean80") return screenScene(80, 24, bs, cleanScreen(), P());
  if (id === "log80") return screenScene(80, 24, bs, logScreen(), P());
  if (id === "peek80") return lazyPeek(bs);
  if (id === "notice80") return noticeScene(bs);
  if (id === "barmod") return barModScene();
  return screenScene(80, 24, bs, listScreen(), P());
}

const LazyScreen = { buildScene: buildScene, toText: toText, toLines: toLines, SCENES: SCENES, THEMES: THEMES, BS: BS, CELLW: CELLW, ROWH: ROWH };
if (typeof window !== "undefined") window.LazyScreen = LazyScreen;
if (typeof module !== "undefined" && module.exports) module.exports = LazyScreen;

// node LAZY_screen.js > LAZY_MOCK.txt
if (typeof require !== "undefined" && typeof module !== "undefined" && require.main === module) {
  const out = ["# lazy.ranma's screen, cell for cell, as this handoff draws it",
    "# (LAZY_screen.js, default theme; colour roles are in the script, not here).",
    "# ranma's tests compare against these. Regenerate only from a new handoff."];
  ["rounded", "none"].forEach(function (bs) {
    SCENES.forEach(function (sc) {
      if (bs !== "rounded" && !(sc.W === 80 && sc.H === 24 && sc.id !== "notice80")) return;
      out.push("## " + sc.title + " · " + bs);
      out.push(toText(buildScene(sc.id, bs)).split("\n").map(function (l) { return l.replace(/ +$/, ""); }).join("\n"));
    });
  });
  console.log(out.join("\n"));
}
