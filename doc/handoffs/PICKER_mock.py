#!/usr/bin/env python3
"""The redrawn picker, cell for cell (card c302).

Lays out every state by the rules in PICKER_MOCK.txt's header, so the mocks
and the rules cannot disagree. Writes PICKER_MOCK.txt (text + role masks) and
mocks.json (the same cells, for the rendered canvas).

Masks, one letter per cell, foreground role:
  b picker_border   f picker_fg   F picker_fg bold   d picker_dim
  a picker_accent   A picker_accent bold   e picker_error bold
  m picker_match (later step)   C the terminal's cursor
  space: a blank cell on the box's ground
  .  not the picker: panes and bar, as they are
Grounds: every cell inside the box (border included) is picker_bg, except the
selected row's inner cells: picker_cursor_bg (marked after the mask line).
"""
import json
import sys

# tl tr bl br h v lt rt tt bt x
SETS = {
    "rounded": "╭╮╰╯─│├┤┬┴┼",
    "plain": "┌┐└┘─│├┤┬┴┼",
    "thick": "┏┓┗┛━┃┣┫┳┻╋",
    "double": "╔╗╚╝═║╠╣╦╩╬",
    "ascii": "++++-|+++++",
    "none": "           ",
}


class Grid:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.ch = [[" "] * w for _ in range(h)]
        self.fg = [["."] * w for _ in range(h)]
        self.bg = [["."] * w for _ in range(h)]  # . terminal / pane ground

    def put(self, x, y, s, fg, bg=None):
        for i, c in enumerate(s):
            if 0 <= x + i < self.w and 0 <= y < self.h:
                self.ch[y][x + i] = c
                self.fg[y][x + i] = fg[i] if len(fg) == len(s) and len(fg) > 1 else fg
                if bg is not None:
                    self.bg[y][x + i] = bg

    def fill(self, x, y, w, h, fg, bg):
        for yy in range(y, y + h):
            self.put(x, yy, " " * w, fg, bg)


def end_cut(s, n):
    if n <= 0:
        return ""
    return s if len(s) <= n else s[: n - 1] + "…"


def mid_cut(s, n):
    if len(s) <= n:
        return s
    if n <= 1:
        return "…"[:n]
    keep = n - 1
    head = keep // 2
    return s[:head] + "…" + s[len(s) - (keep - head):]


# ---- the screen behind ---------------------------------------------------------

CODE = [
    "//! Drawing a frame: pane borders and contents.",
    "",
    "use ratatui::style::{Modifier, Style};",
    "use crate::picker::{Item, Picker};",
    "",
    "fn draw_picker(f: &mut Frame, app: &App) {",
    "    let Some(l) = app.picker_layout() else {",
    "        return;",
    "    };",
    "    let c = &app.config.theme.colors;",
    "    f.render_widget(Clear, rrect(l.outer));",
    "    let block = Block::default()",
    "        .borders(Borders::ALL)",
    "        .border_set(frame_set(&app.config.theme.border))",
    "        .title(Line::from(p.heading()));",
    "    f.render_widget(block, rrect(l.outer));",
    "    let buf = f.buffer_mut();",
    "    let q = l.query;",
    "    if let Some(msg) = &p.message {",
    "        buf.set_stringn(q.x + 1, q.y, msg, q.w as usize, text);",
    "        return;",
    "    }",
    "    let items = p.visible();",
    "    let marks = items.iter().any(|it| it.current);",
    "    for (row, (i, item)) in items.iter().enumerate() {",
    "        let y = l.list.y + row as u16;",
    "        let selected = i == p.selected;",
    "        let (fg, bg) = if selected {",
    "            (c.picker_selected_fg, c.picker_selected_bg)",
    "        } else {",
    "            (c.bar_fg, Color::Reset)",
    "        };",
    "    }",
    "}",
]
SHELL = [
    "7d02e4b picker: mode in the query line",
    "c11a9e0 theme: picker roles follow toast",
    "58be7d1 bar: chip caps",
    "e0c4f3a render: sheet keeps its faces",
    "9b2d6f8 doc: the picker, redrawn",
    "41aa0c7 paste: files over ssh",
    "0de93b2 toast: middle cut for paths",
    "a1f03d2 switch: home and end",
    "",
    "~/projects/ranma main",
    "❯ cargo test picker",
    "running 41 tests",
    "test picker::modes ... ok",
    "test picker::new_session ... ok",
    "test picker::command_line ... ok",
    "test switch::layout ... ok",
    "",
    "test result: ok. 41 passed",
    "",
    "~/projects/ranma main",
    "❯ ",
]
CARGO = [
    "   Compiling ranma v0.9.0 (~/projects/ranma)",
    "    Finished dev [unoptimized] target(s) in 4.81s",
    "     Running unittests src/lib.rs",
    "test layout::dwindle ... ok",
    "test layout::master ... ok",
    "test picker::fuzzy ... ok",
    "test render::sheet ... ok",
    "test theme::fallbacks ... ok",
    "",
    "test result: ok. 412 passed; 0 failed",
]


def pane(g, x, y, w, h, title, active, lines, numbered=False):
    b = "P" if active else "p"
    s = SETS["rounded"]
    g.put(x, y, s[0] + s[4] * (w - 2) + s[1], b)
    g.put(x + 1, y, title, "T" if active else "p")
    for yy in range(y + 1, y + h - 1):
        g.put(x, yy, s[5], b)
        g.put(x + w - 1, yy, s[5], b)
    g.put(x, y + h - 1, s[2] + s[4] * (w - 2) + s[3], b)
    for i in range(h - 2):
        if numbered:
            src = lines[i % len(lines)]
            t = f"{i + 1:>3} " + src
            g.put(x + 1, y + 1 + i, end_cut(t, w - 2)[: w - 2], ".")
            g.put(x + 1, y + 1 + i, f"{i + 1:>3}", ",")
        elif i < len(lines):
            g.put(x + 1, y + 1 + i, lines[i][: w - 2], ".")


def bar(g, y):
    x = 0
    g.put(x, y, " 1:code ", "W", "w")
    x += 8
    g.put(x, y, " 2:web  3 ", "o")
    right = "cpu 12%  mem 41%  21:04 "
    g.put(g.w - len(right), y, right, "q")


def screen(w, h):
    g = Grid(w, h)
    a = h - 1  # the bar is the last row
    if w >= 160:
        lw = w // 2
        pane(g, 0, 0, lw, a, " 1 nvim ", True, CODE, numbered=True)
        top = a // 2
        pane(g, lw, 0, w - lw, top, " 2 zsh ", False, SHELL)
        pane(g, lw, top, w - lw, a - top, " 3 cargo ", False, CARGO)
    elif w >= 60:
        lw = w // 2
        pane(g, 0, 0, lw, a, " 1 nvim ", True, CODE, numbered=True)
        pane(g, lw, 0, w - lw, a, " 2 zsh ", False, SHELL)
    else:
        pane(g, 0, 0, w, a, " 1 zsh ", True, SHELL)
    bar(g, h - 1)
    return g


# ---- the picker -------------------------------------------------------------------

MIN_W, MAX_W, MAX_ROWS = 44, 88, 16


def box_size(W, A, content, rows_open, kind="list", extra_min=0):
    fit = max(min(W - 4, W), 10) if W > 14 else W
    if kind == "list":
        w = max(MIN_W, content + 2, extra_min)
        w = min(w, MAX_W, fit)
        rows = max(1, min(rows_open, MAX_ROWS, max(3, A * 2 // 3 - 4)))
        h = min(rows + 4, A)
        rows = h - 4
    else:
        w = min(max(30, content + 4, extra_min), fit)
        h, rows = 3, 0
    return w, h, rows


def frame(g, x, y, w, h, style, title_parts, footer, ground="k"):
    s = SETS[style]
    g.fill(x, y, w, h, " ", ground)
    g.put(x, y, s[0] + s[4] * (w - 2) + s[1], "b", ground)
    for yy in range(y + 1, y + h - 1):
        g.put(x, yy, s[5], "b", ground)
        g.put(x + w - 1, yy, s[5], "b", ground)
    g.put(x, y + h - 1, s[2] + s[4] * (w - 2) + s[3], "b", ground)
    # The title: from the third cell, as the settings panel has it.
    tx = x + 2
    room = w - 4
    text = " " + "".join(t for t, _ in title_parts) + " "
    if len(text) > room:
        # Narrow: only the active part, cut at its end.
        act = [p for p in title_parts if p[1] in "FA"] or title_parts[:1]
        t = end_cut(act[0][0], room - 2)
        g.put(tx, y, " " + t + " ", " " + act[0][1] * len(t) + " ")
    else:
        g.put(tx, y, " ", " ")
        cx = tx + 1
        for t, role in title_parts:
            g.put(cx, y, t, role)
            cx += len(t)
        g.put(cx, y, " ", " ")
    # The footer: the picker's own keys, right, on the bottom border.
    if footer:
        parts = []
        for k, word in footer:
            parts.append((k, "F"))
            if word:
                parts.append((" " + word, "d"))
            parts.append((" · ", "d"))
        parts = parts[:-1]
        flen = sum(len(t) for t, _ in parts) + 2
        if flen <= w - 4 - 2:
            fx = x + w - 2 - flen
            g.put(fx, y + h - 1, " ", " ")
            cx = fx + 1
            for t, role in parts:
                g.put(cx, y + h - 1, t, role)
                cx += len(t)
            g.put(cx, y + h - 1, " ", " ")


def rule(g, x, y, w, style):
    s = SETS[style]
    g.put(x, y, s[6] + s[4] * (w - 2) + s[7], "b")


def query_row(g, x, y, w, prompt, query, placeholder, count, ascii_=False):
    ix, iw = x + 1, w - 2
    g.put(ix + 1, y, prompt, "A")
    qx = ix + 3
    croom = len(count) + 2 if count else 0
    if count and iw - 3 - len(query) - 1 - croom < 4:
        count, croom = "", 0  # no room: the count goes first
    g.put(qx, y, query, "F")
    cx = qx + len(query)
    g.put(cx, y, "█", "C")
    if not query and placeholder:
        g.put(cx + 1, y, end_cut(placeholder, iw - 3 - 1 - croom - 1), "d")
    if count:
        g.put(ix + iw - 1 - len(count), y, count, "d")


def picker(g, spec):
    W, H = g.w, g.h
    A = H - 1
    style = spec.get("style", "rounded")
    ascii_ = style == "ascii"
    kind = spec.get("kind", "list")
    marks = spec.get("marks", False)
    lead = 2 + (2 if marks else 0)
    cols = spec.get("cols", "column")
    footer = spec.get("footer", [])
    title = spec["title"]
    tlen = sum(len(t) for t, _ in title) + 6
    flen = sum(len(k) + (len(wd) + 1 if wd else 0) + 3 for k, wd in footer) + 3

    if kind == "list":
        if cols == "column":
            lw, dw = spec["label_w"], spec.get("detail_w", 0)
            content = lead + lw + (3 + dw if dw else 0) + 1
        elif cols == "key":
            lw, dw = spec["label_w"], spec["key_w"]
            content = lead + lw + 3 + dw + 1
        elif cols == "help":
            lw = spec["leader_w"] + spec["chord_w"] + 2 + spec["action_w"]
            dw = spec.get("detail_w", 0)
            content = lead + lw + (3 + dw if dw else 0) + 1
        w, h, rows = box_size(W, A, content, spec["rows_open"], "list", max(tlen, flen))
        if spec.get("preview") and W >= 160:
            pw = min(80, max(50, W - 4 - w - 1 - 40))
            w_list = w
            w = w_list + 1 + pw
        else:
            w_list = w
    else:
        content = max(spec.get("text_w", 40), tlen - 4)
        w, h, rows = box_size(W, A, content, 0, kind, max(tlen, flen))
        w_list = w

    x = (W - w) // 2
    y = (A - h) // 3
    frame(g, x, y, w, h, style, title, footer)

    if kind == "question":
        g.put(x + 2, y + 1, end_cut(spec["message"], w - 4), "F")
        return
    ix, iw = x + 1, w_list - 2
    if kind == "prompt":
        query_row(g, x, y + 1, w, spec["prompt"], spec["query"], spec.get("placeholder", ""), "")
        return

    query_row(g, x, y + 1, w_list, spec["prompt"], spec["query"], spec.get("placeholder", ""),
              spec.get("count", ""), ascii_)
    s = SETS[style]
    if spec.get("preview") and W >= 160:
        g.put(x, y + 2, s[6] + s[4] * (w - 2) + s[7], "b")
        px = x + w_list - 1
        g.put(px, y, s[8], "b")
        for yy in range(y + 1, y + h - 1):
            g.put(px, yy, s[5], "b")
        g.put(px, y + 2, s[10], "b")
        g.put(px, y + h - 1, s[9], "b")
        pv = spec["preview"]
        g.put(px + 2, y + 1, end_cut(pv["head"], w - w_list - 3), "d")
        for i, line in enumerate(pv["lines"][: rows]):
            g.put(px + 2, y + 3 + i, line[: w - w_list - 3], "f")
    else:
        rule(g, x, y + 2, w, style)

    items = spec["items"]
    sel = spec.get("sel", 0)
    if cols == "help" and iw < 40:
        # Narrow: the leader is the same on every row; it goes first.
        spec = dict(spec, leader_w=0, chord_w=min(spec["chord_w"], 8), detail_w=0)
        lw = spec["chord_w"] + 2 + spec["action_w"]
        items = [dict(i, leader="") for i in items]
    top = y + 3
    # Column room, shrunk detail first, label last.
    room = iw - lead - 1
    if cols in ("column", "help"):
        base_w = lw
        dwe = spec.get("detail_w", 0)
        if dwe and base_w + 3 + dwe > room:
            dwe = room - 3 - base_w
            if dwe < 8:
                dwe = 0
        lab_w = min(base_w, room - (3 + dwe if dwe else 0))
    else:
        dwe = spec["key_w"]
        lab_w = room - 3 - dwe
    for r, it in enumerate(items[: rows]):
        yy = top + r
        selected = r == sel
        gr = "c" if selected else "k"
        if selected:
            g.put(ix, yy, " " * iw, " ", "c")
            g.put(ix, yy, ">" if ascii_ else "▌", "A")
        lx = ix + lead
        t = it.get("t", "item")
        if marks and it.get("current"):
            g.put(ix + 2, yy, "*" if ascii_ else "●", "a")
        lab_style = "F" if selected else "f"
        if t == "item":
            lab = end_cut(it["label"], lab_w)
            g.put(lx, yy, lab, lab_style)
            if it.get("match"):
                for i in it["match"]:
                    if i < len(lab):
                        g.fg[yy][lx + i] = "m"
            if dwe and it.get("detail"):
                d = it["detail"]
                d = mid_cut(d, dwe) if "/" in d else end_cut(d, dwe)
                if cols == "column":
                    g.put(lx + lab_w + 3, yy, d, "d")
        elif t == "help":
            # Right-aligned against the action, as the which-key hint has it:
            # the leader dim, the chord bold.
            kw = spec["leader_w"] + spec["chord_w"]
            key = (it["leader"] + " " if it["leader"] else "") + it["chord"]
            kx = lx + kw - len(key)
            if it["leader"]:
                g.put(kx, yy, it["leader"], "d")
            g.put(lx + kw - len(it["chord"]), yy, it["chord"], "F")
            ax = lx + kw + 2
            g.put(ax, yy, end_cut(it["action"], lab_w - (ax - lx)), lab_style)
            if dwe and it.get("detail"):
                g.put(lx + lab_w + 3, yy, end_cut(it["detail"], dwe), "d")
        elif t == "cmd":
            name, args = it["name"], it.get("args", "")
            full = end_cut(name + (" " + args if args else ""), lab_w)
            g.put(lx, yy, full, "d")
            g.put(lx, yy, full[: len(name)], lab_style)
            if it.get("key"):
                k = it["key"]
                g.put(ix + iw - 1 - len(k), yy, k, "d")
        elif t == "run":
            g.put(lx, yy, "run", "A")
            g.put(lx + 5, yy, end_cut(it["line"], lab_w - 5), "F")
        elif t == "error":
            g.put(lx, yy, end_cut("! " + it["msg"], lab_w), "e")
        elif t == "new":
            g.put(ix + 2, yy, "+", "A")
            g.put(lx, yy, it["name"], lab_style)
            g.put(lx + lab_w + 3, yy, it["what"], "d")
        elif t == "empty":
            g.put(lx, yy, it["msg"], "d")
    # Where the view is: a thumb on the right border.
    total = spec.get("matches", len(items))
    if total > rows:
        off = spec.get("offset", 0)
        n = max(1, round(rows * rows / total))
        at = min(rows - n, round(off * rows / total))
        glyph = "#" if ascii_ else ("▐" if style == "none" else "┃")
        bx = x + w_list - 1
        for i in range(at, at + n):
            g.put(bx, top + i, glyph, "a")


# ---- the states -------------------------------------------------------------------

PROJECTS = [
    ("ranma", "main:2  ~/projects/", True),
    ("3d-assets", "~/projects/", False),
    ("agents.ranma", "~/projects/", False),
    ("ai-session-board-expansion", "~/projects/", False),
    ("blog", "~/projects/", False),
    ("dotfiles", "~/", False),
    ("hako", "~/projects/", False),
    ("kumiko", "kumiko:1  ~/projects/", False),
    ("matugen-themes", "~/projects/", False),
    ("nvim-config", "~/.config/", False),
    ("poker", "~/projects/", False),
]
PROJ_LW = len("ai-session-board-expansion")
PROJ_DW = len("kumiko:1  ~/projects/")


def projects_items():
    return [{"label": l, "detail": d, "current": c} for l, d, c in PROJECTS]


HELP = [
    ("ctrl+b", "c", "center_column"),
    ("ctrl+b", "q", "close_pane"),
    ("ctrl+b", "shift+e", "column_width full"),
    ("ctrl+b", "e", "column_width next"),
    ("ctrl+b", "{", "consume_or_expel left"),
    ("ctrl+b", "}", "consume_or_expel right"),
    ("ctrl+b", "f", "cycle_floats"),
    ("ctrl+b", "d", "detach"),
    ("ctrl+b", "i", "display_panes"),
    ("ctrl+b", "=", "equalize"),
    ("ctrl+b", "left", "focus left"),
    ("ctrl+b", "down", "focus down"),
    ("ctrl+b", "up", "focus up"),
    ("ctrl+b", "right", "focus right"),
    ("ctrl+b", ";", "focus last"),
    ("ctrl+b", "alt+return", "fullscreen"),
]
HELP_WIDE_EXTRA = [
    ("", "alt+s", "scratchpad_toggle", "global"),
]


def help_items(n, extra=False):
    out = [{"t": "help", "leader": l, "chord": c, "action": a} for l, c, a in HELP[:n]]
    return out


CMDS = [
    ("workspace", "<1-99|next|prev|empty|last>", ""),
    ("workspace_layout", "[dwindle|manual|master|…]", "ctrl+b y w"),
    ("workspace_switcher", "", ""),
    ("move_to_workspace", "<1-99|next|prev|empty|last>", ""),
    ("move_to_workspace_silent", "<1-99|next|prev|…>", ""),
    ("move_workspace_to_session", "", "ctrl+b m"),
    ("rename_workspace", "", "ctrl+b ."),
]


def cmd_items(n):
    return [{"t": "cmd", "name": a, "args": b, "key": k} for a, b, k in CMDS[:n]]


TAB_KEYS = [("? keys", "A"), ("   ", " "), (": commands", "d")]
TAB_CMDS = [("? keys", "d"), ("   ", " "), (": commands", "A")]


def T(t):
    return [(t, "F")]


def states():
    s = []
    # 1 · a plugin's list, as opened: the current item is selected.
    s.append(dict(
        id="projects-open", size=(80, 24),
        name="a plugin's picker (sessionizer), as opened: current item selected",
        note=(
            "Opened on where you are: ranma is current (●) and selected (▌ on "
            "picker_cursor_bg, label bold). The width comes from the widest label and "
            "detail at open (57 with its borders), the height from the item count, capped at 11 rows on "
            "24 lines; it sits a third of the way down. 48 items, 11 shown: the thumb "
            "on the right border says where the view is. A detail that ended in the "
            "label (~/projects/ranma) loses that ending. A plugin picker has no footer: "
            "its keys are the obvious ones."),
        spec=dict(title=T("projects"), prompt="›", query="", placeholder="type to filter",
                  count="48/48", items=projects_items(), sel=0, marks=True,
                  label_w=PROJ_LW, detail_w=PROJ_DW, rows_open=48, matches=48)))
    # 2 · typed, selection moved off the current item.
    typed = [
        {"label": "ranma", "detail": "main:2  ~/projects/", "current": True, "match": [0, 1]},
        {"label": "ranobe", "detail": "~/projects/", "match": [0, 1]},
        {"label": "agents.ranma", "detail": "~/projects/", "match": [7, 8]},
        {"label": "ray-tracer", "detail": "~/src/", "match": [0, 1]},
        {"label": "aurora-wallpapers", "detail": "~/projects/", "match": [3, 4]},
        {"label": "ai-session-board-expansion", "detail": "~/projects/", "match": [17, 18]},
        {"label": "matugen-radix", "detail": "~/projects/", "match": [8, 9]},
        {"label": "terraform-lab", "detail": "~/work/infra/", "match": [4, 5]},
        {"label": "dotfiles-arch", "detail": "~/", "match": [9, 10]},
    ]
    plain = [{k: v for k, v in d.items() if k != "match"} for d in typed]
    s.append(dict(
        id="projects-typed", size=(80, 24),
        name="typed: current not selected, a list shorter than the box",
        note=(
            "Typing keeps the box's size: 9 matches in 11 rows leave 2 blank rows "
            "rather than a box that shrinks under the eye. The selection is on ranobe, "
            "so ranma shows only its ●. Everything fits: no thumb. The count is "
            "matches/all."),
        spec=dict(title=T("projects"), prompt="›", query="ra", count="9/48", items=plain,
                  sel=1, marks=True, label_w=PROJ_LW, detail_w=PROJ_DW, rows_open=48,
                  matches=9)))
    # 3 · nothing matches.
    s.append(dict(
        id="projects-empty", size=(80, 24),
        name="empty: nothing matches",
        note="Same box; one dim row says so. No selection, no thumb.",
        spec=dict(title=T("projects"), prompt="›", query="zzq", count="0/48",
                  items=[{"t": "empty", "msg": "nothing matches"}], sel=-1, marks=True,
                  label_w=PROJ_LW, detail_w=PROJ_DW, rows_open=48, matches=0)))
    # 4 · the session switcher, a new one from the query, its own keys.
    s.append(dict(
        id="sessions-new", size=(80, 24),
        name="sessions: a new session from the query, Ctrl+R in the footer",
        note=(
            "A switcher whose list can grow by one (sessions, workspaces) reserves "
            "that row at open. The `+ new session` row: + in accent, the words dim, "
            "the name as a label. The detail drops ' · current' (the ● says it). The "
            "footer shows only keys that are not obvious: ^R rename, then esc."),
        spec=dict(title=T("sessions"), prompt="›", query="po", count="1/5",
                  items=[{"label": "poker", "detail": "3 panes"},
                         {"t": "new", "what": "new session", "name": "po"}],
                  sel=0, marks=True, label_w=6, detail_w=11, rows_open=6, matches=2,
                  footer=[("^R", "rename"), ("esc", "")])))
    # 5 · help.
    s.append(dict(
        id="help", size=(80, 24),
        name="help (? keys): the key is a column of its own",
        note=(
            "The title is the two modes, the active one in accent: it says `:` switches "
            "without a sentence. The prompt glyph is the typed prefix. Keys: the leader "
            "dim, the chord bold, the whole key right-aligned against the action as the "
            "which-key hint draws it, so the eye runs down one edge. ↵ runs the bind: said in the footer."),
        spec=dict(title=TAB_KEYS, prompt="?", query="", placeholder="a key, or what it does",
                  count="96/96", items=help_items(11), sel=0, cols="help",
                  leader_w=7, chord_w=16, action_w=22, detail_w=6, rows_open=96, matches=96,
                  footer=[("↵", "runs it"), ("esc", "")])))
    # 6 · commands, a typed argument: run row.
    s.append(dict(
        id="commands-run", size=(80, 24),
        name="commands (: …) with an argument: the run row",
        note=(
            "`run` in accent bold, the parsed line bold: it is what Enter does. The "
            "argument hint after a name is dim; a bound key is right-aligned, as menus "
            "put shortcuts (the `key` variation)."),
        spec=dict(title=TAB_CMDS, prompt=":", query="workspace 3", count="7/118",
                  items=[{"t": "run", "line": "workspace 3"}] + cmd_items(6), sel=0,
                  cols="key", label_w=46, key_w=10, rows_open=118, matches=7,
                  footer=[("tab", "completes"), ("↵", "runs"), ("esc", "")])))
    # 7 · commands, an error.
    s.append(dict(
        id="commands-error", size=(80, 24),
        name="commands: the parser's error heads the list",
        note="The error row is picker_error (bar_urgent) and bold, never selected by Enter's effect: Enter on it stays open, as today.",
        spec=dict(title=TAB_CMDS, prompt=":", query="worksapce 3", count="0/118",
                  items=[{"t": "error", "msg": "unknown action `worksapce`"}], sel=0,
                  cols="key", label_w=46, key_w=10, rows_open=118, matches=1,
                  footer=[("tab", "completes"), ("↵", "runs"), ("esc", "")])))
    # 8 · servers.
    s.append(dict(
        id="servers", size=(80, 24),
        name="servers: the current one, Ctrl+X in the footer",
        note=(
            "The title loses its parenthesis: what Enter and Ctrl+X do moves to the "
            "footer. The detail is long; it is cut at its end (not a path) before the "
            "label is touched."),
        spec=dict(title=T("servers"), prompt="›", query="", placeholder="type to filter",
                  count="3/3",
                  items=[{"label": "default", "detail": "this one · 9 panes · main, kumiko · now", "current": True},
                         {"label": "work", "detail": "attached elsewhere · 4 panes · infra · 2h ago"},
                         {"label": "vps", "detail": "detached · 2 panes · irc · 3d ago · other build"}],
                  sel=0, marks=True, label_w=7, detail_w=48, rows_open=3,
                  footer=[("↵", "move here"), ("^X", "kill"), ("esc", "")])))
    # 9 · a prompt.
    s.append(dict(
        id="prompt", size=(80, 24),
        name="a prompt (rename session): one line, no list",
        note="Three rows, as today. The parenthesised hints of the rename prompts become footer words (`empty clears`).",
        spec=dict(kind="prompt", title=T("rename session main"), prompt="›", query="main",
                  footer=[("↵", "rename"), ("esc", "")])))
    # 10 · a question.
    s.append(dict(
        id="question", size=(80, 24),
        name="a question (quit ranma): a message, one key",
        note="No prompt glyph, no cursor. The answer keys leave the message for the footer: `y quit · any other key stays`.",
        spec=dict(kind="question", title=T("quit ranma"), message="Close 9 panes in 2 sessions?",
                  text_w=40, footer=[("y", "quits"), ("any other key", "stays")])))
    # 11 · ascii.
    s.append(dict(
        id="projects-ascii", size=(80, 24), style="ascii",
        name="border.style = ascii",
        note="Every glyph has an ASCII stand-in: ▌ is >, ● is *, the thumb is # (as panes.scrollbar), › stays (it is the prompt, text).",
        spec=dict(title=T("projects"), prompt=">", query="", placeholder="type to filter",
                  count="48/48", items=projects_items(), sel=2, marks=True, style="ascii",
                  label_w=PROJ_LW, detail_w=PROJ_DW, rows_open=48, matches=48, offset=0)))
    # 12 · thick (the thumb on a heavy border).
    s.append(dict(
        id="projects-thick", size=(80, 24), style="thick",
        name="border.style = thick, scrolled to the middle",
        note="On a heavy border the thumb keeps the glyph and differs by colour (picker_accent over picker_border). Scrolled: the selection is the last row, the thumb mid-way.",
        spec=dict(title=T("projects"), prompt="›", query="", placeholder="type to filter",
                  count="48/48",
                  items=[{"label": l, "detail": d} for l, d in [
                      ("nvim-config", "~/.config/"), ("poker", "~/projects/"),
                      ("ranobe", "~/projects/"), ("ray-tracer", "~/src/"),
                      ("recipes", "~/notes/"), ("scratch", "~/tmp/"),
                      ("sessionizer.ranma", "~/projects/"), ("sumi-theme", "~/projects/"),
                      ("terraform-lab", "~/work/infra/"), ("toast-demo", "~/projects/"),
                      ("wallpapers", "~/pictures/")]],
                  sel=10, marks=True, style="thick", label_w=PROJ_LW, detail_w=PROJ_DW,
                  rows_open=48, matches=48, offset=19)))
    # 13 · narrow, nested.
    s.append(dict(
        id="narrow", size=(28, 12),
        name="28×12: a nested ranma in a narrow pane",
        note=(
            "Under 44 columns the box is the screen less 4. What goes, in order: the "
            "footer, the detail (under 8 cells it is dropped, not shown as `~/…`), the "
            "inactive mode tab, the count. The label is cut last, at its end."),
        spec=dict(title=TAB_KEYS, prompt="?", query="", placeholder="a key, or what it does",
                  count="96/96", items=help_items(7), sel=0, cols="help",
                  leader_w=7, chord_w=16, action_w=22, detail_w=6, rows_open=96, matches=96,
                  footer=[("↵", "runs it"), ("esc", "")])))
    # 14 · wide: pane switcher.
    s.append(dict(
        id="wide-panes", size=(200, 50),
        name="200×50: the pane switcher",
        note=(
            "Wide does not mean big: the box is as wide as its content (here the minimum, 44) and at "
            "most 16 rows, a third of the way down. The pane switcher starts at the top "
            "with the focused pane only marked."),
        spec=dict(title=T("panes"), prompt="›", query="", placeholder="type to filter",
                  count="9/9",
                  items=[{"label": a, "detail": b, "current": c} for a, b, c in [
                      ("nvim", "main · 1", True), ("zsh", "main · 1", False),
                      ("cargo", "main · 1", False), ("htop", "main · 2", False),
                      ("lazygit", "main · 3", False), ("nvim", "kumiko · 1", False),
                      ("psql", "kumiko · 2", False), ("weechat", "vps · 1", False),
                      ("notes", "scratchpad", False)]],
                  sel=0, marks=True, label_w=12, detail_w=12, rows_open=9)))
    # 15 · wide: help.
    s.append(dict(
        id="wide-help", size=(200, 50),
        name="200×50: help, 16 rows, a global bind",
        note="The height caps at 16 rows on a tall screen: a launcher, not a page. A global bind has no leader cell and a dim `global` detail.",
        spec=dict(title=TAB_KEYS, prompt="?", query="", placeholder="a key, or what it does",
                  count="96/96",
                  items=help_items(15) + [{"t": "help", "leader": "", "chord": "alt+s",
                                           "action": "scratchpad_toggle", "detail": "global"}],
                  sel=3, cols="help", leader_w=7, chord_w=16, action_w=22, detail_w=6,
                  rows_open=96, matches=96, offset=0,
                  footer=[("↵", "runs it"), ("esc", "")])))
    # 16 · later: preview + matched letters.
    s.append(dict(
        id="wide-preview", size=(200, 50),
        name="later: a preview beside the list, matched letters",
        note=(
            "A later step, not part of the first change. On a screen 160+ wide, the "
            "pane and layout switchers may add a panel: the selected pane's last lines "
            "(`ranma capture`), plain picker_fg. The divider joins the rule (┬ ┼ ┴). "
            "Matched letters in picker_match (unset: picker_accent) need fuzzy_score to "
            "return positions."),
        spec=dict(title=T("panes"), prompt="›", query="ca", count="3/9",
                  items=[{"label": "cargo", "detail": "main · 1", "match": [0, 1]},
                         {"label": "lazygit", "detail": "main · 3", "match": [2, 5]},
                         {"label": "scratch", "detail": "scratchpad", "match": [3, 5]}],
                  sel=0, marks=True, label_w=12, detail_w=12, rows_open=9, matches=3,
                  preview=dict(head="cargo · ~/projects/ranma", lines=CARGO))))
    return s


def build(st):
    W, H = st["size"]
    g = screen(W, H)
    spec = dict(st["spec"])
    spec.setdefault("style", st.get("style", "rounded"))
    picker(g, spec)
    return g


def before():
    """Today's drawing (draw_picker), for the canvas's before/after only."""
    W, H = 80, 24
    g = screen(W, H)
    A = H
    w = min(W - 4, 72)
    h = min(max(A * 3 // 5, 5), 48 + 3)
    x, y = (W - w) // 2, (A - h) // 3
    s = SETS["rounded"]
    g.fill(x, y, w, h, " ", "t")
    g.put(x, y, s[0] + s[4] * (w - 2) + s[1], "M", "t")
    g.put(x + 1, y, " projects ", "o")
    for yy in range(y + 1, y + h - 1):
        g.put(x, yy, s[5], "M")
        g.put(x + w - 1, yy, s[5], "M")
    g.put(x, y + h - 1, s[2] + s[4] * (w - 2) + s[3], "M")
    g.put(x + 1, y + 1, "> ", "o")
    g.put(x + 3, y + 1, "█", "C")
    full = [(l, (d + l if d.endswith("/") and d != "~/" else d + l)) for l, d, _ in PROJECTS]
    full[0] = ("ranma", "main:2  ~/projects/ranma")
    for r, (l, d) in enumerate(full[: h - 3]):
        yy = y + 2 + r
        sel = r == 0
        iw = w - 2
        if sel:
            g.put(x + 1, yy, " " * iw, "S", "s")
        lab = (" ● " if r == 0 else "   ") + l
        g.put(x + 1, yy, lab, "S" if sel else "o")
        dd = d + " "
        if len(lab) + len(dd) + 2 <= iw:
            g.put(x + 1 + iw - len(dd), yy, dd, "S" if sel else "q")
    return g


def mask_txt(g):
    out = []
    for y in range(g.h):
        m = "".join(
            "." if g.bg[y][x] not in "kc" else g.fg[y][x] for x in range(g.w)
        )
        line = m
        if any(g.bg[y][x] == "c" for x in range(g.w)):
            line += "  ◂ picker_cursor_bg"
        out.append(line)
    return out


HEADER = open(sys.argv[1]).read() if len(sys.argv) > 1 else ""


def main():
    out = [HEADER.rstrip("\n"), ""]
    data = []
    for st in states():
        g = build(st)
        W, H = st["size"]
        style = st.get("style", "rounded")
        lines = ["".join(r) for r in g.ch]
        for l in lines:
            assert len(l) == W, (st["id"], len(l), l)
        out.append(f"## {W}x{H} · {st['name']} · {style}")
        out.append("# " + st["note"])
        out.extend(lines)
        out.append(f"### roles: {st['id']}")
        out.extend(mask_txt(g))
        out.append("")
        data.append(dict(id=st["id"], w=W, h=H, name=st["name"], style=style,
                         lines=lines, fg=["".join(r) for r in g.fg],
                         bg=["".join(r) for r in g.bg]))
    g = before()
    data.insert(0, dict(id="before", w=80, h=24, name="today", style="rounded",
                        lines=["".join(r) for r in g.ch], fg=["".join(r) for r in g.fg],
                        bg=["".join(r) for r in g.bg]))
    open("PICKER_MOCK.txt", "w").write("\n".join(out) + "\n")
    json.dump(data, open("mocks.json", "w"), ensure_ascii=False, separators=(",", ":"))


if __name__ == "__main__":
    main()
