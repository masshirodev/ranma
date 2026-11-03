# Design brief: ranma's website, ranma.masshiro.pro

```
Repository: https://github.com/masshirodev/ranma (public). It's my repository and my
GitHub account is connected to claude.ai, so attach it to this session
yourself with read-only access (you never need to write to it), then make a
shallow clone and read the code. A plain `git clone` or `gh` has no
credentials here, so don't start there. If attaching is refused, ask me once
to approve it rather than designing from this brief alone, and ask me for the
files only if that fails too.
```

Read first: [`README.md`](../../README.md), whole: it is the current pitch,
the feature list and the install table, and the site says the same things
better rather than different things; [`doc/DESIGN.md`](../DESIGN.md), "Why
not tuios" and "Non-goals" (what ranma refuses to be is part of the pitch);
[`doc/CONFIG.md`](../CONFIG.md) and the default config
[`assets/init.lua`](../../assets/init.lua); the default theme
[`assets/themes/Default.toml`](../../assets/themes/Default.toml) (its
colours are ranma's own, a reasonable place to start the site's palette);
[`doc/screenshot.png`](../screenshot.png) and [`doc/demo.gif`](../demo.gif),
the only visuals that exist today; [`doc/ROADMAP.md`](../ROADMAP.md) for
what is done; and `doc/man/` for the reference the site links to rather than
copies.

This is the text to forward to **Claude Design**. It's a brief, not a
handoff. Card c409 ("A website at ranma.masshiro.pro") on the Ranma board.

---

## Why

ranma is a public project with a release pipeline (apt, dnf, AUR, Homebrew,
Nix, crates.io, npm) and no front door. The README does the job on GitHub,
but a link shared anywhere else lands on a code host. The site is that front
door: one page someone can read in a minute and leave either with ranma
installed or knowing it isn't for them.

## Who reads it

Terminal people: they already use tmux, zellij, or a tiling WM like i3,
Hyprland or niri, and they read a config file before a feature list. They
are suspicious of marketing. Show them the thing working, then the
keystrokes and the Lua, then how to install. Assume a dark-mode default but
design the light theme too.

## What the site has

One page is the core; design it first. Anything past it is optional and
should be argued for on the canvas, not assumed.

1. **What it is**, in a sentence, with the name's meaning (欄間, the carved
   transom panel above sliding doors). That image is a gift for the visual
   identity: a lattice above a doorway, panels framed in wood. Use it or
   argue against it, but consider it.
2. **It working.** The demo is the strongest thing ranma has. Today it is a
   GIF; the build can record it again as a video (or an asciinema-style
   cast) if the design calls for that, so design for the best medium and
   say which one.
3. **What it does**, grouped the way the README groups it: tiling (dwindle,
   tabbed groups, floating layer, scratchpad, scrolling columns), sessions
   and workspaces (the server, detach and reattach, fuzzy switchers), copy
   mode and the clipboard over SSH, images and the kitty protocols, the bar,
   the mobile view. Each group wants a short line and, where it earns it, a
   still or a short loop. Mark which visuals the build has to capture; it can
   capture any of them from a real ranma in a headless terminal.
4. **Configured in Lua, themed in TOML**: a real excerpt of `init.lua` and of
   a theme, highlighted, not a paraphrase.
5. **Why not tmux / zellij / tuios**, briefly and fairly. DESIGN.md's tuios
   table is the source; keep the tone of that document (it refuses scope, it
   does not out-engineer anyone).
6. **Install**: the one-line `curl … | sh` first, then the per-platform table
   (apt, dnf, AUR, Homebrew, Nix, cargo, npm, by hand). Every command must be
   copyable with one click. The apt and dnf snippets are long; decide how
   they are shown without hiding them.
7. **Footer**: GitHub, releases, the man pages and CONFIG.md, the changelog,
   the licence (MIT or Apache-2.0), "made by masshirodev".

## Constraints

- **Static.** The site is plain files built from `site/` in the repository
  and served as they are; nothing runs on the server. No analytics, no
  trackers, no third-party fonts or scripts at runtime. Any font is
  self-hosted.
- **It must work at phone width** (a 16px side gutter, no horizontal scroll)
  and with JavaScript off, except for copy buttons and theme toggles.
- **The install URLs do not change.** `install.sh`, the apt and dnf
  repositories and the signing key stay at `masshirodev.github.io/ranma`,
  which installed machines already point at. The site shows those URLs.
- **Monospace is the native type** of the subject, but a page set entirely in
  it reads like a README. Choose deliberately.
- Light and dark themes, following the system and toggleable.
- The version number and the latest release come from the repository at
  build time; design a place for them, never a hand-typed number.

## Out of scope

- tsundoku's site: a separate brief in its own repository.
- Moving the apt/dnf repositories or `install.sh` off GitHub Pages.
- A docs site. The man pages and CONFIG.md stay where they are and the site
  links to them. If you think a rendered docs section is worth it, say so on
  the canvas as a later step.
- A blog, a newsletter, a sign-up of any kind.

---

When the design is done, append ` [finished]` to the artifact's title, and
end the page with a line reading `Status: finished`. Nothing tells the agent
waiting on this that a design is ready; it looks for that mark.
