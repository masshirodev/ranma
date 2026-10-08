
const CELLW = 9;

const BS = {
  rounded: { tl: "╭", tr: "╮", bl: "╰", br: "╯", h: "─", v: "│", lt: "├", rt: "┤", tt: "┬", bt: "┴", x: "┼" },
  plain:   { tl: "┌", tr: "┐", bl: "└", br: "┘", h: "─", v: "│", lt: "├", rt: "┤", tt: "┬", bt: "┴", x: "┼" },
  thick:   { tl: "┏", tr: "┓", bl: "┗", br: "┛", h: "━", v: "┃", lt: "┣", rt: "┫", tt: "┳", bt: "┻", x: "╋" },
  double:  { tl: "╔", tr: "╗", bl: "╚", br: "╝", h: "═", v: "║", lt: "╠", rt: "╣", tt: "╦", bt: "╩", x: "╬" },
  ascii:   { tl: "+", tr: "+", bl: "+", br: "+", h: "-", v: "|", lt: "+", rt: "+", tt: "+", bt: "+", x: "+" },
  none:    { tl: " ", tr: " ", bl: " ", br: " ", h: " ", v: " ", lt: " ", rt: " ", tt: " ", bt: " ", x: " " }
};
const ORDER = ["rounded", "plain", "thick", "double", "ascii", "none"];

const THEMES = {
  default: {
    bg: "#1e1e2e", fg: "#cdd6f4", surface: "#313244",
    border_active: "#89b4fa", border_inactive: "#45475a", border_floating: "#f5c2e7",
    bar_bg: "default", bar_fg: "#cdd6f4", bar_dim: "#6c7086", bar_accent: "#89b4fa", bar_urgent: "#f38ba8",
    mode_fg: "#1e1e2e", mode_bg: "#f38ba8",
    ws_active_fg: "#1e1e2e", ws_active_bg: "#89b4fa", ws_occupied: "#cdd6f4", ws_empty: "#6c7086", ws_urgent: "#f38ba8",
    tab_active_fg: "#1e1e2e", tab_active_bg: "#89b4fa", tab_inactive_fg: "#cdd6f4", tab_inactive_bg: "#313244",
    picker_selected_fg: "#1e1e2e", picker_selected_bg: "#89b4fa",
    search_fg: "#1e1e2e", search_bg: "#f9e2af", search_current_fg: "#1e1e2e", search_current_bg: "#fab387",
    toast_fg: "#cdd6f4", toast_bg: "#313244",
    red: "#f38ba8", green: "#a6e3a1", yellow: "#f9e2af", blue: "#89b4fa", magenta: "#cba6f7", cyan: "#94e2d5", gray: "#6c7086", orange: "#fab387"
  },
  matugen: {
    bg: "#151d19", fg: "#dce5df", surface: "#26312b",
    border_active: "#8bd6b4", border_inactive: "#3c4842", border_floating: "#a8cbe0",
    bar_bg: "default", bar_fg: "#dce5df", bar_dim: "#86948c", bar_accent: "#8bd6b4", bar_urgent: "#ffb4ab",
    mode_fg: "#0f1f26", mode_bg: "#a8cbe0",
    ws_active_fg: "#00382a", ws_active_bg: "#8bd6b4", ws_occupied: "#dce5df", ws_empty: "#86948c", ws_urgent: "#ffb4ab",
    tab_active_fg: "#00382a", tab_active_bg: "#8bd6b4", tab_inactive_fg: "#dce5df", tab_inactive_bg: "#26312b",
    picker_selected_fg: "#00382a", picker_selected_bg: "#8bd6b4",
    search_fg: "#1f1600", search_bg: "#e8c983", search_current_fg: "#1f1600", search_current_bg: "#f2b88a",
    toast_fg: "#dce5df", toast_bg: "#26312b",
    red: "#ffb4ab", green: "#a3d39c", yellow: "#e8c983", blue: "#9ccaff", magenta: "#d7bde4", cyan: "#8fd0d6", gray: "#86948c", orange: "#f2b88a"
  },
  latte: {
    bg: "#eff1f5", fg: "#4c4f69", surface: "#dce0e8",
    border_active: "#1e66f5", border_inactive: "#bcc0cc", border_floating: "#8839ef",
    bar_bg: "default", bar_fg: "#4c4f69", bar_dim: "#7c7f93", bar_accent: "#1e66f5", bar_urgent: "#d20f39",
    mode_fg: "#eff1f5", mode_bg: "#8839ef",
    ws_active_fg: "#eff1f5", ws_active_bg: "#1e66f5", ws_occupied: "#4c4f69", ws_empty: "#8c8fa1", ws_urgent: "#d20f39",
    tab_active_fg: "#eff1f5", tab_active_bg: "#1e66f5", tab_inactive_fg: "#4c4f69", tab_inactive_bg: "#dce0e8",
    picker_selected_fg: "#eff1f5", picker_selected_bg: "#1e66f5",
    search_fg: "#4c4f69", search_bg: "#df8e1d", search_current_fg: "#eff1f5", search_current_bg: "#fe640b",
    toast_fg: "#4c4f69", toast_bg: "#dce0e8",
    red: "#d20f39", green: "#40a02b", yellow: "#df8e1d", blue: "#1e66f5", magenta: "#8839ef", cyan: "#179299", gray: "#8c8fa1", orange: "#fe640b"
  }
};

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
function isSafe(ch) {
  const c = ch.codePointAt(0);
  return (c >= 0x20 && c < 0x7f) || (c >= 0x2500 && c <= 0x259f);
}

class Scr {
  constructor(W, H) {
    this.W = W; this.H = H; this.clip = null;
    this.cells = [];
    for (let i = 0; i < W * H; i++) this.cells.push({ ch: " ", fg: "fg", bg: "bg", b: false, it: false, u: false, f: 0 });
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
    c.b = !!st.b; c.it = !!st.it; c.u = !!st.u; c.f = st.f || 0;
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
}

function toLines(s, T) {
  const out = [];
  for (let y = 0; y < s.H; y++) {
    const spans = [];
    let cur = null;
    for (let x = 0; x < s.W; x++) {
      const c = s.cells[y * s.W + x];
      const safe = isSafe(c.ch);
      const bg = res(T, c.bg);
      let fg = res(T, c.fg);
      if (c.f) fg = mix(fg, bg, c.f);
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

/* ---------- the option registry ---------- */

const GROUPS = [
  { id: "general", name: "General" }, { id: "wm", name: "WM mode" }, { id: "looks", name: "Looks" },
  { id: "colours", name: "Colours" }, { id: "paste", name: "Paste" },
  { id: "mpris", name: "mpris", plugin: true }, { id: "battery", name: "battery", plugin: true }
];

const COLOR_ROLES = [
  ["border_active", "Border active", "The focused pane’s border."],
  ["border_inactive", "Border inactive", "Every other pane’s border."],
  ["border_floating", "Border floating", "The border of floats and popups."],
  ["bar_bg", "Bar bg", "The bar’s ground. default: the terminal’s own background."],
  ["bar_fg", "Bar fg", "Bar text, and the normal module style."],
  ["bar_dim", "Bar dim", "The dim module style, and quiet text in pickers and this panel."],
  ["bar_accent", "Bar accent", "The accent module style, toast borders, and the marks in this panel."],
  ["bar_urgent", "Bar urgent", "The urgent module style, and urgent toasts."],
  ["mode_fg", "Mode fg", "The text of the mode chip on the bar."],
  ["mode_bg", "Mode bg", "The mode chip, the focused border in WM mode, and the border of pickers and this panel."],
  ["ws_active_fg", "Current ws fg", "The current workspace in the workspaces module: text."],
  ["ws_active_bg", "Current ws bg", "The current workspace in the workspaces module: ground."],
  ["ws_occupied", "Occupied ws", "A workspace with panes in it."],
  ["ws_empty", "Empty ws", "A workspace with nothing in it."],
  ["ws_urgent", "Urgent ws", "A workspace with a bell or an urgent pane."],
  ["tab_active_fg", "Active tab fg", "The current tab of a grouped container: text."],
  ["tab_active_bg", "Active tab bg", "The current tab of a grouped container: ground."],
  ["tab_inactive_fg", "Tab fg", "The other tabs: text."],
  ["tab_inactive_bg", "Tab bg", "The other tabs: ground."],
  ["picker_selected_fg", "Picker selected fg", "The selected row’s text in pickers, help and this panel."],
  ["picker_selected_bg", "Picker selected bg", "The selected row’s ground in pickers, help and this panel."],
  ["search_fg", "Search fg", "A search match in copy mode: text."],
  ["search_bg", "Search bg", "A search match in copy mode: ground."],
  ["search_current_fg", "Search current fg", "The current match: text."],
  ["search_current_bg", "Search current bg", "The current match: ground."],
  ["toast_fg", "Toast fg", "Toast text, and the text of this panel."],
  ["toast_bg", "Toast bg", "The ground of toasts and of this panel."]
];

function buildReg(T, tn, bs) {
  const R = [];
  const add = function (o) { R.push(o); };
  add({ key: "leader", group: "general", name: "Leader", type: "string", def: "ctrl+b", file: "init.lua", desc: "The chord that enters WM mode. Enter, then press the chord itself." });
  add({ key: "theme", group: "general", name: "Theme", type: "enum", choices: ["default", "matugen", "latte"], def: "default", init: tn !== "default" ? tn : undefined, file: "init.lua", desc: "The theme file under themes/. Every colour in Colours starts from it." });
  add({ key: "layout", group: "general", name: "Layout", type: "enum", def: "dwindle", init: "master", file: "init.lua", desc: "Where a new pane goes: dwindle splits the focused pane; master keeps one pane on the left and stacks the rest." });
  add({ key: "master_ratio", group: "general", name: "Master ratio", type: "num", min: 0.1, max: 0.9, step: 0.05, fmt: "pct", def: 0.55, init: 0.6, file: "init.lua", desc: "With layout master: the master pane’s share of the width, when a master area forms." });
  add({ key: "preserve_split", group: "general", name: "Preserve split", type: "bool", def: true, file: "init.lua", desc: "Keep a split’s direction when the workspace is resized." });
  add({ key: "shell", group: "general", name: "Shell", type: "string", def: null, unsetLabel: "$SHELL", file: "init.lua", desc: "The program new panes run. Unset: $SHELL, then /bin/sh." });
  add({ key: "scrollback_lines", group: "general", name: "Scrollback", type: "num", slider: false, min: 0, max: 100000, step: 1000, fmt: "int", def: 10000, file: "init.lua", desc: "Lines of scrollback kept per pane." });
  add({ key: "mouse", group: "general", name: "Mouse", type: "enum", def: "click", init: "hover", file: "init.lua", desc: "Outside WM mode: click focuses the pane clicked, hover the pane under the pointer, off leaves the mouse to the terminal." });
  add({ key: "restore", group: "general", name: "Restore", type: "enum", def: "ask", file: "init.lua", desc: "ask: a fresh server offers back the last snapshot of itself. off: no snapshots, no question." });
  add({ key: "splash", group: "general", name: "Splash", type: "bool", def: true, init: false, file: "init.lua", desc: "An empty workspace shows the ranma logo, with the keys to start below it." });
  add({ key: "wm_mode.sticky", group: "wm", name: "Sticky", type: "bool", def: true, file: "init.lua", desc: "Stay in WM mode until Esc or Enter. Off: every bind is one-shot." });
  add({ key: "wm_mode.hint", group: "wm", name: "Hint delay", type: "num", min: 0, max: 2, step: 0.1, fmt: "sec", def: 0.5, file: "init.lua", desc: "The pause in WM mode before the which-key hint shows. All the way left is off." });
  add({ key: "border.style", group: "looks", name: "Border style", type: "enum", def: "rounded", init: bs !== "rounded" ? bs : undefined, file: "theme", desc: "The line pane borders, floats and pickers are drawn with." });
  add({ key: "border.floating_style", group: "looks", name: "Float border", type: "enum", def: "same", file: "theme", desc: "The border of floats and popups. same: the border style above." });
  add({ key: "border.title", group: "looks", name: "Title position", type: "enum", def: "top", file: "theme", desc: "Where a pane’s title sits on its border: top, bottom, or off." });
  add({ key: "border.title_align", group: "looks", name: "Title align", type: "enum", def: "left", file: "theme", desc: "left, center or right, along the border." });
  add({ key: "border.title_format", group: "looks", name: "Title format", type: "string", def: " {title} ", init: " {index}[ {program}] ", file: "theme", desc: "A pane’s title: {title}, {index}, {program}, {cwd}. A part in [ ] shows only when its placeholders all have values." });
  add({ key: "border.indicator", group: "looks", name: "Indicator", type: "enum", def: "none", file: "theme", desc: "arrows: marks on the focused pane’s edges, pointing in." });
  add({ key: "gaps.inner", group: "looks", name: "Inner gap", type: "num", min: 0, max: 8, step: 1, fmt: "int", def: 0, panel: 1, file: "theme", desc: "Cells between two panes." });
  add({ key: "gaps.outer_horizontal", group: "looks", name: "Outer gap, sides", type: "num", min: 0, max: 16, step: 1, fmt: "int", def: 0, file: "theme", desc: "Cells between the panes and the left and right edges. A cell is about twice as tall as wide." });
  add({ key: "gaps.outer_vertical", group: "looks", name: "Outer gap, ends", type: "num", min: 0, max: 8, step: 1, fmt: "int", def: 0, file: "theme", desc: "Cells between the panes and the top and bottom edges." });
  add({ key: "panes.dim_unfocused", group: "looks", name: "Dim unfocused", type: "num", min: 0, max: 1, step: 0.05, fmt: "pct", def: 0, init: 0.3, panel: 0.5, file: "theme", desc: "How far the text of panes you are not in fades toward their background. 0 is off." });
  add({ key: "panes.active_bg", group: "looks", name: "Focused pane bg", type: "color", def: null, file: "theme", desc: "The focused pane’s ground, where its program leaves the default background. Unset: the terminal’s own." });
  add({ key: "panes.inactive_bg", group: "looks", name: "Other panes bg", type: "color", def: null, file: "theme", desc: "The other panes’ ground. Unfocused text fades toward it." });
  add({ key: "bar.position", group: "looks", name: "Bar position", type: "enum", def: "bottom", file: "theme", desc: "top, bottom, or hidden." });
  add({ key: "bar.separator", group: "looks", name: "Bar separator", type: "string", def: "  ", file: "theme", desc: "Drawn between two modules on the same side of the bar." });
  add({ key: "bar.workspace_format", group: "looks", name: "Workspace format", type: "string", def: " {n}[:{name}] ", file: "theme", desc: "A workspace in the workspaces module: {n} its number, {name} its name or its program’s." });
  COLOR_ROLES.forEach(function (r) {
    add({ key: "colors." + r[0], role: r[0], group: "colours", name: r[1], type: "color", def: T[r[0]], file: "theme", desc: r[2] });
  });
  add({ key: "paste.upload", group: "paste", name: "Upload over ssh", type: "bool", def: true, file: "init.lua", desc: "A paste of local file paths into a pane running ssh uploads the files and types the far paths." });
  add({ key: "paste.image_command", group: "paste", name: "Image command", type: "string", def: null, unsetLabel: "auto", file: "init.lua", desc: "A shell command that writes the clipboard’s image as PNG to stdout. Unset: chosen for your system." });
  add({ key: "mpris.format", group: "mpris", name: "Format", type: "string", def: " {artist} – {title} ", file: "init.lua", desc: "What the module shows while something plays." });
  add({ key: "mpris.max_width", group: "mpris", name: "Max width", type: "num", min: 10, max: 80, step: 2, fmt: "int", def: 40, init: 32, file: "init.lua", desc: "Cells before the text is cut with …" });
  add({ key: "mpris.paused", group: "mpris", name: "Show when paused", type: "bool", def: false, file: "init.lua", desc: "Keep the module on the bar while playback is paused." });
  add({ key: "battery.warn", group: "battery", name: "Warn below", type: "num", min: 5, max: 50, step: 5, fmt: "ipct", def: 20, file: "init.lua", desc: "Charge, in percent, under which the module turns urgent." });
  add({ key: "battery.hide_full", group: "battery", name: "Hide when full", type: "bool", def: true, file: "init.lua", desc: "Leave the bar alone while on mains power and full." });
  return R;
}

function saved(o) { return o.panel !== undefined ? o.panel : o.init !== undefined ? o.init : o.def; }
function eff(o) { return o.pend !== undefined ? o.pend : saved(o); }
function markOf(o) {
  if (o.pend !== undefined && o.pend !== saved(o)) return "*";
  if (o.panel !== undefined && o.init !== undefined && o.panel !== o.init) return "◆";
  if (saved(o) !== o.def) return "•";
  return "";
}
function fmtNum(o, v) {
  if (o.fmt === "pct") return Math.round(v * 100) + "%";
  if (o.fmt === "sec") return v <= 0 ? "off" : v.toFixed(1) + "s";
  if (o.fmt === "ipct") return v + "%";
  return String(v);
}
function fmtV(o, v) {
  if (o.type === "num") return fmtNum(o, v);
  if (o.type === "bool") return v ? "on" : "off";
  if (o.type === "color") return v || "unset";
  if (o.type === "string") return v == null ? (o.unsetLabel || "unset") : "\"" + v + "\"";
  return String(v);
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
  return lines;
}

/* ---------- one option row ---------- */

function drawRow(s, x0, y, CW, o, sel, ctx) {
  ctx = ctx || {};
  const wide = CW >= 60, compact = CW < 28;
  const base = sel ? { fg: "picker_selected_fg" } : { fg: "toast_fg" };
  const dim = sel ? { fg: "picker_selected_fg", f: 0.4 } : { fg: "bar_dim" };
  const acc = sel ? { fg: "picker_selected_fg", b: true } : { fg: "bar_accent" };
  if (sel) s.fill(x0 - 1, y, CW + 2, 1, { bg: "picker_selected_bg" });
  s.put(x0, y, sel ? "›" : " ", { fg: sel ? "picker_selected_fg" : "bar_accent", b: true });
  const v = eff(o);
  const segs = [];
  const P = function (t, st) { segs.push([t, st]); };
  const sp = compact ? "" : " ";
  if (ctx.edit !== undefined) {
    const ok = /^#[0-9a-fA-F]{6}$/.test(ctx.edit);
    P("[" + sp, acc); P("██", { fg: ok ? ctx.edit : v }); P(" ", base);
    const fld = { fg: "toast_fg", bg: "toast_bg" };
    P(ctx.edit, fld); P(" ", { fg: "toast_bg", bg: "toast_fg" });
    const pad = Math.max(0, 8 - len(ctx.edit));
    if (pad) P(" ".repeat(pad), fld);
    P(sp + "]", acc);
  } else if (o.type === "enum") {
    P("‹" + sp, acc); P(String(v), base); P(sp + "›", acc);
  } else if (o.type === "bool") {
    P("[" + sp, dim); P(v ? "on" : "off", v ? Object.assign({}, acc, { b: true }) : base); P(sp + "]", dim);
  } else if (o.type === "num") {
    const sw = o.slider === false ? 0 : (CW >= 60 ? 17 : CW >= 40 ? 7 : CW >= 34 ? 5 : 0);
    if (sw) {
      const fr = (v - o.min) / (o.max - o.min);
      const k = Math.round(fr * (sw - 1));
      for (let i = 0; i < sw; i++) {
        if (i < k) P("━", sel ? { fg: "picker_selected_fg" } : { fg: "bar_accent" });
        else if (i === k) P("●", sel ? { fg: "picker_selected_fg", b: true } : { fg: "toast_fg", b: true });
        else P("─", dim);
      }
      P(" ", base);
    }
    const t = fmtNum(o, v);
    if (!compact && len(t) < 4) P(" ".repeat(4 - len(t)), base);
    P("‹" + sp, acc); P(t, base); P(sp + "›", acc);
  } else if (o.type === "color") {
    P("[" + sp, dim);
    if (!v || v === "default") { P("··", dim); P(" " + (v || "unset"), dim); }
    else { P("██", { fg: v }); P(" " + v, base); }
    P(sp + "]", dim);
  } else {
    const maxIn = Math.max(4, CW - (wide ? 44 : 22));
    let d = v == null ? (o.unsetLabel || "unset") : "\"" + v + "\"";
    d = trunc(d, maxIn);
    P("[" + sp, dim); P(d, v == null ? dim : base); P(sp + "]", dim);
  }
  const vw = segs.reduce(function (a, sg) { return a + len(sg[0]); }, 0);
  const vx = x0 + CW - vw;
  const nameMax = Math.min(wide ? 24 : 22, vx - (x0 + 2) - 3);
  const nm = trunc(o.name, nameMax);
  const q = (ctx.query || "").toLowerCase();
  const mi = q ? o.name.toLowerCase().indexOf(q) : -1;
  const na = Array.from(nm);
  for (let i = 0; i < na.length; i++) {
    const hit = mi >= 0 && i >= mi && i < mi + q.length && na[i] !== "…";
    s.set(x0 + 2 + i, y, na[i], Object.assign({}, base, { u: hit, b: hit }));
  }
  const mk = markOf(o);
  if (mk) s.put(x0 + 2 + na.length + 1, y, mk, mk === "•" ? base : Object.assign({}, acc, { b: true }));
  if (wide) {
    const src = o.pend !== undefined ? "unsaved" : o.panel !== undefined ? "panel" : o.init !== undefined ? o.file : "";
    if (src) s.put(x0 + 28, y, src, src === "unsaved" ? acc : dim);
  }
  let x = vx;
  segs.forEach(function (sg) { x = s.put(x, y, sg[0], sg[1]); });
}

function drawHead(s, x0, y, CW, g, shown, total, q, hch) {
  let x = s.put(x0, y, g.name, { fg: "bar_accent", b: true });
  if (g.plugin) x = s.put(x + 1, y, "plugin", { fg: "bar_dim" });
  const cnt = q ? shown + " of " + total : String(total);
  const end = x0 + CW - len(cnt) - 1;
  for (x = x + 1; x < end; x++) s.set(x, y, hch, { fg: "bar_dim" });
  s.put(x0 + CW - len(cnt), y, cnt, { fg: "bar_dim" });
}

function buildList(reg, q) {
  q = (q || "").toLowerCase();
  const out = [];
  GROUPS.forEach(function (g) {
    const all = reg.filter(function (o) { return o.group === g.id; });
    const opts = q ? all.filter(function (o) { return o.name.toLowerCase().indexOf(q) >= 0; }) : all;
    if (!opts.length) return;
    out.push({ head: true, g: g, total: all.length, shown: opts.length });
    opts.forEach(function (o) { out.push({ o: o }); });
  });
  return out;
}

function footKeys(o, mode, wide) {
  if (mode === "confirm") return [["w", "save"], ["d", "discard"], ["esc", "keep editing"]];
  if (mode === "edit") return [["enter", "apply"], ["esc", "cancel"], ["ctrl+u", "clear"]];
  if (mode === "filter") return wide ? [["↑↓", "move"], ["←→", "change"], ["enter", "back to list"], ["esc", "clear filter"]] : [["↑↓", "move"], ["←→", "change"], ["esc", "clear"]];
  let k;
  if (o.type === "enum") k = [["←→", "choose"]];
  else if (o.type === "bool") k = [["←→", "flip"]];
  else if (o.type === "num") k = [["←→", "step"], ["enter", "type"]];
  else if (o.type === "color") k = [["←→", "theme colours"], ["enter", "type"]];
  else k = [["enter", "edit"]];
  k.push(["r", "default"]);
  if (wide) { k.push(["u", "undo"]); k.push(["/", "filter"]); k.push(["tab", "next group"]); k.push(["space", "peek"]); }
  k.push(["?", "keys"]);
  return k;
}
function putKeys(s, x, y, keys, maxX) {
  keys.forEach(function (kv, i) {
    const need = len(kv[0]) + 1 + len(kv[1]) + (i ? 2 : 0);
    if (x + need > maxX) return;
    if (i) x += 2;
    x = s.put(x, y, kv[0], { fg: "bar_accent", b: true });
    x = s.put(x + 1, y, kv[1], { fg: "bar_dim" });
  });
  return x;
}

/* ---------- the panel ---------- */

function drawPanel(s, T, P) {
  const x = P.x, y = P.y, w = P.w, h = P.h, bs = P.bs, B = BS[bs];
  const none = bs === "none";
  const bst = { fg: "mode_bg", bg: none ? "toast_bg" : "bg" };
  const rst = { fg: "border_inactive", bg: "toast_bg" };
  const hch = bs === "ascii" ? "-" : "─";
  s.fill(x, y, w, h, { bg: "toast_bg", fg: "toast_fg" });
  s.box(x, y, w, h, bs, bst);
  const wide = w >= 90;
  const IX = wide ? 22 : 0;
  const sepX = x + 1 + IX;
  const x0 = wide ? sepX + 2 : x + 2;
  const CW = (x + w - 2) - x0;
  const D = wide ? 3 : 2;
  const qY = y + 1, r1 = y + 2, listTop = y + 3;
  const footY = y + h - 2, rule2 = footY - 1, provY = rule2 - 1, descTop = provY - D, rule1 = descTop - 1;
  const L = rule1 - listTop;
  const hRule = function (yy, j) {
    for (let i = x + 1; i < x + w - 1; i++) s.set(i, yy, B.h, rst);
    s.set(x, yy, B.lt, bst); s.set(x + w - 1, yy, B.rt, bst);
    if (wide && j) s.set(sepX, yy, j, rst);
  };
  hRule(r1, B.x); hRule(rule1, B.bt); hRule(rule2, null);
  if (wide) {
    for (let yy = y + 1; yy < rule1; yy++) if (yy !== r1) s.set(sepX, yy, B.v, rst);
    s.set(sepX, y, B.tt, bst);
  }

  const reg = P.reg;
  const list = buildList(reg, P.query);
  const total = reg.length;
  let selIdx = list.findIndex(function (it) { return it.o && it.o.key === P.sel; });
  if (selIdx < 0) selIdx = list.findIndex(function (it) { return !!it.o; });
  const selO = list[selIdx].o;

  // title, unsaved count, bottom labels
  s.put(x + 2, y, " settings ", { fg: "toast_fg", bg: bst.bg, b: true });
  const unsaved = reg.filter(function (o) { return o.pend !== undefined; });
  if (unsaved.length) {
    const t = " " + unsaved.length + " unsaved ";
    s.put(x + w - 2 - len(t) - (wide ? 0 : 0), y, t, { fg: "bar_accent", bg: bst.bg, b: true });
  }
  const bl = [[" ", null], ["w", "k"], [" save · ", null], ["esc", "k"], [" close ", null]];
  let bx = x + w - 2 - bl.reduce(function (a, p) { return a + len(p[0]); }, 0);
  bl.forEach(function (p) { bx = s.put(bx, y + h - 1, p[0], p[1] ? { fg: "bar_accent", bg: bst.bg, b: true } : { fg: "bar_dim", bg: bst.bg }); });

  // query row
  if (P.query) {
    let qx = s.put(x0, qY, "/", { fg: "bar_accent", b: true });
    qx = s.put(qx + 1, qY, P.query, { fg: "toast_fg", b: true });
    if (P.filtering) s.set(qx, qY, " ", { bg: "toast_fg" });
    const n = list.filter(function (it) { return !!it.o; }).length;
    const t = n + " of " + total;
    s.put(x0 + CW - len(t), qY, t, { fg: "bar_dim" });
  } else {
    let qx = s.put(x0, qY, "/", { fg: "bar_dim", b: true });
    s.put(qx + 1, qY, "filter", { fg: "bar_dim" });
    const t = total + " options";
    s.put(x0 + CW - len(t), qY, t, { fg: "bar_dim" });
  }

  // group index (wide)
  if (wide) {
    s.put(x + 2, qY, "Groups", { fg: "bar_dim" });
    let gy = listTop;
    let sepDone = false;
    GROUPS.forEach(function (g) {
      if (g.plugin && !sepDone) {
        sepDone = true;
        gy += 1;
        let px = s.put(x + 2, gy, "plugins ", { fg: "bar_dim" });
        for (; px < sepX - 1; px++) s.set(px, gy, hch, { fg: "border_inactive" });
        gy += 1;
      }
      const all = reg.filter(function (o) { return o.group === g.id; });
      const shown = P.query ? all.filter(function (o) { return o.name.toLowerCase().indexOf(P.query) >= 0; }).length : all.length;
      const chg = all.filter(function (o) { return markOf(o) !== ""; }).length;
      const cur = selO.group === g.id;
      const faded = P.query && !shown;
      s.put(x + 2, gy, cur ? "›" : " ", { fg: "bar_accent", b: true });
      s.put(x + 4, gy, g.name, cur ? { fg: "toast_fg", b: true } : faded ? { fg: "bar_dim" } : { fg: "toast_fg" });
      if (chg) s.put(x + 15, gy, ("•" + chg).padStart(3), { fg: "bar_dim" });
      const cnt = P.query ? String(shown) : String(all.length);
      s.put(sepX - 1 - len(cnt), gy, cnt, { fg: "bar_dim" });
      gy += 1;
    });
  }

  // list
  let off = 0;
  const headIdx = (function () { for (let i = selIdx; i >= 0; i--) if (list[i].head) return i; return 0; })();
  off = headIdx;
  if (selIdx - off >= L) off = selIdx - L + 3;
  off = Math.max(0, Math.min(off, Math.max(0, list.length - L)));
  if (!list.length) s.put(x0, listTop, "nothing matches", { fg: "bar_dim" });
  for (let i = 0; i < L; i++) {
    const it = list[off + i];
    if (!it) break;
    const yy = listTop + i;
    if (it.head) drawHead(s, x0, yy, CW, it.g, it.shown, it.total, P.query, hch);
    else drawRow(s, x0, yy, CW, it.o, off + i === selIdx, { query: P.query, edit: off + i === selIdx ? P.edit : undefined });
  }
  if (list.length > L) {
    const th = Math.max(1, Math.round(L * L / list.length));
    const tp = Math.round(off / (list.length - L) * (L - th));
    const tch = bs === "thick" ? "█" : bs === "ascii" ? "#" : bs === "none" ? "▐" : "┃";
    for (let i = 0; i < th; i++) s.set(x + w - 1, listTop + tp + i, tch, { fg: "toast_fg", bg: bst.bg });
  }

  // description, sources, keys
  const mode = P.confirm ? "confirm" : P.edit !== undefined ? "edit" : P.filtering ? "filter" : "list";
  if (mode === "confirm") {
    s.put(x0, descTop, "Close with " + unsaved.length + " unsaved changes?", { fg: "toast_fg", b: true });
    unsaved.slice(0, D).forEach(function (o, i) {
      let cx = s.put(x0, descTop + 1 + i, trunc(o.name, 16), { fg: "toast_fg" });
      cx = Math.max(cx + 1, x0 + 17);
      cx = s.put(cx, descTop + 1 + i, fmtV(o, saved(o)), { fg: "bar_dim" });
      cx = s.put(cx + 1, descTop + 1 + i, "→", { fg: "bar_accent", b: true });
      s.put(cx + 1, descTop + 1 + i, fmtV(o, o.pend), { fg: "toast_fg", b: true });
    });
  } else if (mode === "edit") {
    wrap("A hex #rrggbb, an ANSI name (blue, bright-black), 0–255, or default.", CW, D).forEach(function (ln, i) {
      s.put(x0, descTop + i, ln, { fg: "toast_fg" });
    });
    let cx = s.put(x0, provY, "not a colour yet", { fg: "bar_urgent", b: true });
    cx = s.put(cx, provY, " · was ", { fg: "bar_dim" });
    cx = s.put(cx, provY, "██", { fg: eff(selO) });
    s.put(cx + 1, provY, String(eff(selO)), { fg: "bar_dim" });
  } else {
    const isPanelRole = selO.role && /^(picker_selected|toast)/.test(selO.role);
    const dl = wrap(selO.desc, CW, isPanelRole && selO.pend !== undefined ? D - 1 : D);
    dl.forEach(function (ln, i) { s.put(x0, descTop + i, ln, { fg: "toast_fg" }); });
    if (isPanelRole && selO.pend !== undefined) {
      const yy = descTop + D - 1;
      let cx = s.put(x0, yy, "sample", { fg: "bar_dim" });
      cx += 2;
      const r = selO.role;
      const sfg = r === "picker_selected_fg" ? selO.pend : r === "toast_fg" ? selO.pend : r.indexOf("toast") === 0 ? "toast_fg" : "picker_selected_fg";
      const sbg = r === "picker_selected_bg" ? selO.pend : r === "toast_bg" ? selO.pend : r.indexOf("toast") === 0 ? "toast_bg" : "picker_selected_bg";
      const sw = Math.min(40, x0 + CW - cx);
      s.fill(cx, yy, sw, 1, { bg: sbg });
      s.put(cx, yy, "› Dim unfocused", { fg: sfg, b: false });
      s.put(cx + sw - 9, yy, "‹  50% ›", { fg: sfg });
      s.put(cx + sw + 2, yy, "the panel keeps its colours while open", { fg: "bar_dim" });
    }
    // where the value comes from
    const parts = [["default", fmtV(selO, selO.def), selO.init === undefined && selO.panel === undefined]];
    if (selO.init !== undefined) parts.push([selO.file, fmtV(selO, selO.init), selO.panel === undefined]);
    if (selO.panel !== undefined) parts.push(["panel", fmtV(selO, selO.panel), true]);
    let cx = x0;
    parts.forEach(function (p, i) {
      if (i) cx = s.put(cx, provY, " · ", { fg: "bar_dim" });
      cx = s.put(cx, provY, p[0] + " ", { fg: "bar_dim" });
      cx = s.put(cx, provY, trunc(p[1], wide ? 30 : 14), p[2] ? { fg: "toast_fg", b: true } : { fg: "bar_dim" });
    });
    if (selO.pend !== undefined) {
      cx = s.put(cx + 1, provY, "→", { fg: "bar_accent", b: true });
      cx = s.put(cx + 1, provY, fmtV(selO, selO.pend), { fg: "bar_accent", b: true });
      if (wide) s.put(cx + 1, provY, "unsaved", { fg: "bar_dim" });
    }
    if (wide) {
      const kt = selO.key + " · " + (selO.type === "num" ? (selO.fmt === "int" || selO.fmt === "ipct" ? "int " : "float ") + fmtNum(selO, selO.min) + "–" + fmtNum(selO, selO.max) : selO.type);
      s.put(x0 + CW - len(kt), provY, kt, { fg: "bar_dim" });
    }
  }
  putKeys(s, x0, footY, footKeys(selO, mode, wide), x0 + CW);

  if (P.help) drawHelp(s, x + Math.floor((w - 38) / 2), y + 2, bs);
  return selO;
}

function drawHelp(s, x, y, bs) {
  const w = 38, h = 19;
  s.fill(x, y, w, h, { bg: "bg", fg: "toast_fg" });
  s.box(x, y, w, h, bs, { fg: "bar_accent", bg: "bg" });
  s.put(x + 2, y, " keys and marks ", { fg: "toast_fg", bg: "bg", b: true });
  const keys = [
    ["↑↓ j k", "move"], ["←→ h l", "change the value"], ["enter", "type a value, or flip"],
    ["r", "back to the default"], ["u", "undo the unsaved edit"], ["/", "filter by name"],
    ["tab", "next group"], ["space", "peek at the panes"], ["w", "save to settings.toml"], ["esc", "close; asks if unsaved"]
  ];
  keys.forEach(function (k, i) {
    s.put(x + 2, y + 1 + i, k[0], { fg: "bar_accent", b: true });
    s.put(x + 11, y + 1 + i, k[1], { fg: "toast_fg" });
  });
  const my = y + 12;
  let px = s.put(x + 2, my, "marks ", { fg: "bar_dim" });
  for (; px < x + w - 2; px++) s.set(px, my, bs === "ascii" ? "-" : "─", { fg: "border_inactive" });
  const marks = [["•", "differs from the default", false], ["◆", "the panel’s value wins over", true], ["", "init.lua or the theme", false], ["*", "changed, not saved yet", true]];
  marks.forEach(function (m, i) {
    if (m[0]) s.put(x + 3, my + 1 + i, m[0], m[2] ? { fg: "bar_accent", b: true } : { fg: "toast_fg" });
    s.put(x + 6, my + 1 + i, m[1], { fg: "toast_fg" });
  });
  s.put(x + 2, y + h - 1, " any key ", { fg: "bar_dim", bg: "bg" });
}

/* ---------- the workspace behind ---------- */

const KW = /^(pub|fn|let|mut|use|for|in|if|return|break|struct|impl|self|match|else|const|crate)$/;
function rustLine(src) {
  if (/^\s*\/\//.test(src)) return [[src, "gray", true]];
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
const CODE = [
  "//! Layouts: a tree of splits.",
  "",
  "use crate::pane::PaneId;",
  "",
  "pub fn dwindle(area: Rect, n: usize) -> Vec<Rect> {",
  "    let mut out = Vec::new();",
  "    let mut rest = area;",
  "    for i in 0..n {",
  "        if i + 1 == n {",
  "            out.push(rest);",
  "            break;",
  "        }",
  "        let (a, b) = split(rest, i);",
  "        out.push(a);",
  "        rest = b;",
  "    }",
  "    out",
  "}",
  "",
  "pub fn master(area: Rect, n: usize, ratio: f32) -> Vec<Rect> {",
  "    let w = (area.w as f32 * ratio).round() as u16;",
  "    let (m, stack) = area.split_x(w);",
  "    let mut out = vec![m];",
  "    out.extend(stack.rows(n - 1));",
  "    out",
  "}",
  "",
  "fn split(r: Rect, i: usize) -> (Rect, Rect) {",
  "    if i % 2 == 0 { r.split_x(r.w / 2) } else { r.split_y(r.h / 2) }",
  "}",
  "",
  "#[cfg(test)]",
  "mod tests {",
  "    use super::*;",
  "",
  "    #[test]",
  "    fn dwindle_fills_the_area() {",
  "        let rs = dwindle(Rect::new(0, 0, 80, 24), 3);",
  "        assert_eq!(rs.len(), 3);",
  "    }",
  "}"
];
function nvim(s, x, y, w, h, f) {
  for (let i = 0; i < h - 1 && i < CODE.length; i++) {
    let cx = s.put(x, y + i, String(i + 1).padStart(3) + " ", { fg: i === 8 ? "yellow" : "gray", f: f });
    rustLine(CODE[i]).forEach(function (sg) { cx = s.put(cx, y + i, sg[0], { fg: sg[1], it: !!sg[2], f: f }); });
  }
  for (let i = CODE.length; i < h - 1; i++) s.put(x, y + i, "~", { fg: "blue", f: f });
  s.fill(x, y + h - 1, w, 1, { bg: "surface" });
  let cx = s.put(x, y + h - 1, " NORMAL ", { fg: "bg", bg: "blue", b: true, f: f });
  cx = s.put(cx, y + h - 1, " layout.rs ", { fg: "fg", f: f });
  if (w >= 26) s.put(x + w - 7, y + h - 1, " 9:13 ", { fg: "fg", f: f });
}
const ZSH = [
  [["~/src/ranma", "cyan"], [" main", "magenta"]],
  [["❯ ", "green"], ["git log --oneline -8", "fg"]],
  [["a3f91c2 ", "yellow"], ["layout: master keeps its ratio", "fg"]],
  [["7d02e4b ", "yellow"], ["picker: headings stay on filter", "fg"]],
  [["c11a9e0 ", "yellow"], ["theme: toast roles for panels", "fg"]],
  [["58be7d1 ", "yellow"], ["bar: mode chip for sub-modes", "fg"]],
  [["e0c4f3a ", "yellow"], ["render: crop panes, never resize", "fg"]],
  [["9b2d6f8 ", "yellow"], ["doc: plugins in Lua", "fg"]],
  [["41aa0c7 ", "yellow"], ["paste: upload over ssh", "fg"]],
  [["0de93b2 ", "yellow"], ["toast: urgent border", "fg"]],
  [["~/src/ranma", "cyan"], [" main", "magenta"]],
  [["❯ ", "green"], ["█", "fg"]]
];
const CARGO = [
  [["$ ", "gray"], ["cargo test -q", "fg"]],
  [["running 42 tests", "fg"]],
  [["test layout::dwindle ... ", "fg"], ["ok", "green"]],
  [["test layout::master ... ", "fg"], ["ok", "green"]],
  [["test picker::filter ... ", "fg"], ["ok", "green"]],
  [["test theme::inherit ... ", "fg"], ["ok", "green"]],
  [["test render::crop ... ", "fg"], ["ok", "green"]],
  [["test paste::upload ... ", "fg"], ["ok", "green"]],
  [["test toast::stack ... ", "fg"], ["ok", "green"]],
  [["", "fg"]],
  [["test result: ", "fg"], ["ok", "green"], [". 42 passed; 0 failed", "fg"]],
  [["$ ", "gray"], ["█", "fg"]]
];
function segLines(lines) {
  return function (s, x, y, w, h, f) {
    const start = Math.max(0, lines.length - h);
    for (let i = start; i < lines.length; i++) {
      let cx = x;
      lines[i].forEach(function (sg) { cx = s.put(cx, y + i - start, sg[0], { fg: sg[1], f: f }); });
    }
  };
}
function fmtTitle(fmt, p) {
  return fmt.replace(/\{index\}/g, p.idx).replace(/\{program\}/g, p.prog).replace(/\{title\}/g, p.title).replace(/\{cwd\}/g, "~/src/ranma").replace(/[\[\]]/g, "");
}
function drawWorkspace(s, R, L) {
  const g = L.gap;
  const wA = Math.round((R.w - g) * L.ratio);
  const wB = R.w - wA - g;
  const hB = Math.floor((R.h - g) / 2);
  const hC = R.h - hB - g;
  const panes = [
    { x: R.x, y: R.y, w: wA, h: R.h, focus: true, idx: 1, prog: "nvim", title: "layout.rs", draw: nvim },
    { x: R.x + wA + g, y: R.y, w: wB, h: hB, idx: 2, prog: "zsh", title: "~/src/ranma", draw: segLines(ZSH) },
    { x: R.x + wA + g, y: R.y + hB + g, w: wB, h: hC, idx: 3, prog: "cargo", title: "cargo test", draw: segLines(CARGO) }
  ];
  panes.forEach(function (p) {
    const bfg = p.focus ? L.ba : "border_inactive";
    s.box(p.x, p.y, p.w, p.h, L.bs, { fg: bfg, bg: "bg" });
    s.clip = { x: p.x + 1, y: p.y + 1, w: p.w - 2, h: p.h - 2 };
    p.draw(s, p.x + 1, p.y + 1, p.w - 2, p.h - 2, p.focus ? 0 : L.dim);
    s.clip = { x: p.x + 1, y: p.y, w: p.w - 2, h: 1 };
    s.put(p.x + 1, p.y, fmtTitle(L.tf, p), { fg: bfg, bg: "bg", b: p.focus });
    s.clip = null;
  });
}
function drawBar(s, y, W) {
  s.fill(0, y, W, 1, { fg: "bar_fg", bg: "bar_bg" });
  let x = s.put(0, y, " SET ", { fg: "mode_fg", bg: "mode_bg", b: true });
  x = s.put(x + 2, y, " 1:code ", { fg: "ws_active_fg", bg: "ws_active_bg", b: true });
  x = s.put(x, y, " 2:web ", { fg: "ws_occupied" });
  x = s.put(x, y, " 3 ", { fg: "ws_empty" });
  const r1 = "cpu 12%  mem 41%  ", r2 = "16:36 ";
  s.put(W - len(r1) - len(r2), y, r1, { fg: "bar_dim" });
  s.put(W - len(r2), y, r2, { fg: "bar_fg" });
}

/* ---------- scenes ---------- */

const SCN = {
  help80: { sel: "panes.dim_unfocused", help: true },
  enum80: { sel: "border.style", pend: function (T, bs) { return { "border.style": bs === "thick" ? "double" : "thick" }; } },
  bool80: { sel: "splash" },
  slider80: { sel: "panes.dim_unfocused", pend: function () { return { "panes.dim_unfocused": 0.6 }; } },
  colour80: { sel: "colors.border_active", pend: function (T) { return { "colors.border_active": T.search_current_bg }; } },
  string80: { sel: "border.title_format" },
  filter80: { sel: "border.style", query: "bor", filtering: true },
  edit80: { sel: "colors.border_active", edit: "#fab38" },
  esc80: { sel: "panes.dim_unfocused", confirm: true, pend: function (T) { return { "panes.dim_unfocused": 0.6, "colors.border_active": T.search_current_bg }; } },
  peek80: { sel: "panes.dim_unfocused", peek: true, pend: function () { return { "panes.dim_unfocused": 0.6 }; } },
  wide: { W: 200, H: 50 }
};
const WIDE = {
  slider: { sel: "panes.dim_unfocused", pend: function () { return { "panes.dim_unfocused": 0.6, "gaps.inner": 2 }; } },
  enum: { sel: "border.style", pend: function (T, bs) { return { "border.style": bs === "thick" ? "double" : "thick" }; } },
  bool: { sel: "preserve_split" },
  colour: { sel: "colors.picker_selected_bg", pend: function (T) { return { "colors.picker_selected_bg": T.search_current_bg }; } },
  string: { sel: "border.title_format" }
};

function liveOf(byKey) {
  return {
    bs: eff(byKey["border.style"]), gap: eff(byKey["gaps.inner"]), dim: eff(byKey["panes.dim_unfocused"]),
    ratio: eff(byKey["master_ratio"]), ba: eff(byKey["colors.border_active"]), tf: eff(byKey["border.title_format"])
  };
}

function buildScene(id, T, tn, bs, selType) {
  if (id === "shrink") return shrinkScene(T, tn, bs);
  const cfg = Object.assign({ W: 80, H: 24 }, SCN[id] || SCN.help80);
  if (cfg.W >= 160) Object.assign(cfg, WIDE[selType] || WIDE.slider);
  const reg = buildReg(T, tn, bs);
  const byKey = {};
  reg.forEach(function (o) { byKey[o.key] = o; });
  const pend = cfg.pend ? cfg.pend(T, bs) : {};
  Object.keys(pend).forEach(function (k) { byKey[k].pend = pend[k]; });
  const W = cfg.W, H = cfg.H;
  const s = new Scr(W, H);
  const live = liveOf(byKey);
  if (cfg.peek) {
    drawWorkspace(s, { x: 0, y: 0, w: W, h: H - 4 }, live);
    drawPeek(s, bs, byKey[cfg.sel], W, H, Object.keys(pend).length);
  } else {
    const pw = W >= 160 ? 96 : W >= 100 ? 56 : W >= 64 ? 44 : W;
    if (W - pw > 0) drawWorkspace(s, { x: 0, y: 0, w: W - pw, h: H - 1 }, live);
    drawPanel(s, T, { x: W - pw, y: 0, w: pw, h: H - 1, bs: bs, reg: reg, sel: cfg.sel, query: cfg.query, filtering: cfg.filtering, edit: cfg.edit, confirm: cfg.confirm, help: cfg.help });
  }
  drawBar(s, H - 1, W);
  return s;
}

function drawPeek(s, bs, o, W, H, n) {
  const y = H - 4;
  const none = bs === "none";
  const bst = { fg: "mode_bg", bg: none ? "toast_bg" : "bg" };
  s.fill(0, y, W, 3, { bg: "toast_bg", fg: "toast_fg" });
  s.box(0, y, W, 3, bs, bst);
  s.put(2, y, " settings · peek ", { fg: "toast_fg", bg: bst.bg, b: true });
  if (n) { const t = " " + n + " unsaved "; s.put(W - 2 - len(t), y, t, { fg: "bar_accent", bg: bst.bg, b: true }); }
  drawRow(s, 2, y + 1, W - 4, o, true, {});
  const bl = [[" ", 0], ["space", 1], [" back · ", 0], ["←→", 1], [" step · ", 0], ["esc", 1], [" close ", 0]];
  let bx = W - 2 - bl.reduce(function (a, p) { return a + len(p[0]); }, 0);
  bl.forEach(function (p) { bx = s.put(bx, y + 2, p[0], p[1] ? { fg: "bar_accent", bg: bst.bg, b: true } : { fg: "bar_dim", bg: bst.bg }); });
}

function shrinkScene(T, tn, bs) {
  const W = 100, H = 26;
  const s = new Scr(W, H);
  const reg = buildReg(T, tn, bs);
  const byKey = {};
  reg.forEach(function (o) { byKey[o.key] = o; });
  byKey["panes.dim_unfocused"].pend = 0.6;
  const rows = [
    [69, "69 cells · the 200×50 panel: slider 17, source column"],
    [40, "40 cells · the 80×24 panel: slider 7"],
    [34, "34 cells · slider 5"],
    [28, "28 cells · no slider; the value and mark stay"],
    [22, "22 cells · inner spaces go, then names give way"]
  ];
  rows.forEach(function (r, k) {
    const y = 1 + k * 5, cw = r[0];
    s.put(2, y, r[1], { fg: "bar_dim" });
    s.fill(2, y + 1, cw + 4, 3, { bg: "toast_bg", fg: "toast_fg" });
    drawRow(s, 4, y + 1, cw, byKey["panes.dim_unfocused"], true, {});
    drawRow(s, 4, y + 2, cw, byKey["colors.border_active"], false, {});
    drawRow(s, 4, y + 3, cw, byKey["border.title_format"], false, {});
  });
  return s;
}

class Component extends DCLogic {
  renderVals() {
    const scene = this.props.scene ?? "help80";
    const tn = THEMES[this.props.theme] ? this.props.theme : "default";
    const bs = BS[this.props.borderStyle] ? this.props.borderStyle : "rounded";
    const selType = this.props.selType ?? "slider";
    const T = THEMES[tn];
    const s = buildScene(scene, T, tn, bs, selType);
    return { lines: toLines(s, T), pxW: s.W * CELLW, pxH: s.H * 18, bgc: T.bg, fgc: T.fg };
  }
}
