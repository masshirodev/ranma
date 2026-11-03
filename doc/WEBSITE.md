# The website: ranma.masshiro.pro

One page that says what ranma is, shows it, and gets it installed. Its source is
[`site/`](../site); it is built into plain files and served by Caddy on the
`masshiro.pro` VPS. Card c409 ("A website at ranma.masshiro.pro") on the Ranma
board; the brief is [`briefs/WEBSITE.md`](briefs/WEBSITE.md) and the design
handoff is mirrored in [`handoffs/website/`](handoffs/website).

The site is not where ranma is installed from. `install.sh`, the apt and dnf
repositories and the signing key stay on GitHub Pages
(`masshirodev.github.io/ranma`, [`RELEASING.md`](RELEASING.md)), because
installed machines already point there. The page shows those URLs; it never
copies the files.

## How it builds

```sh
cd site
npm ci                # dev tools only: Playwright, Prettier, two font packages
npm run build         # → site/build/
npm run serve         # http://localhost:4173/, plain files as Caddy serves them
```

`build.mjs` has no dependencies. It reads:

| What the page says        | Where it comes from                                                                            |
| ------------------------- | ---------------------------------------------------------------------------------------------- |
| The version and its date  | the newest `v*` tag reachable from HEAD (`git describe`), not Cargo.toml, which runs ahead       |
| The `init.lua` excerpt    | `doc/examples/screenshot/init.lua`, lines 10-18 and 33-46                                       |
| The `sumi.toml` excerpt   | `doc/examples/screenshot/themes/sumi.toml`, lines 1-16 and 46-51                                |
| The demo's chapters       | `site/static/media/demo.vtt`, written from the `# chapter:` lines in `scripts/demo/demo.tape`  |
| The pictures and videos   | `site/static/media/`, committed (below)                                                         |

Each excerpt pins the start of its first line, so editing the example config
fails the build ("line 10 reads …") instead of quietly quoting the wrong lines.
Move the range in `build.mjs` when that happens.

Every asset is written under a content-hashed name (`site.696dfb7191.css`) so
Caddy can let browsers keep it for a year; `index.html` is never cached. The
page sets no inline style and runs no inline script, so the CSP can forbid both
(the swatches in the theme excerpt are classes the build writes into the
stylesheet for that reason).

## Captures

Pictures and videos are made by a script and **committed**; a deploy never
records anything. That was the plan the owner approved on 2026-11-03: a deploy
stays fast, a half-finished UI never reaches the site by accident, and a new
capture shows up as a diff to review. The cost is that the page shows ranma as
of the last capture, so re-record after a change that shows.

```sh
site/scripts/captures.sh stills   # doc/screenshot.png → screenshot.webp, og.jpg
site/scripts/captures.sh videos   # vhs: demo, sessions and copy-mode loops
site/scripts/captures.sh kitty    # how to take the two stills only kitty can show
```

- **`videos`** needs what [`scripts/demo/README.md`](../scripts/demo/README.md)
  lists (vhs, ttyd, a Chromium, btop, nvim, git) and ffmpeg with libsvtav1 and
  libx264. It records in the staged home the demo gif uses, on a private ranma
  server, so no real session is touched. Each tape becomes a WebM (AV1, for
  browsers that have it), an MP4 (H.264, for the rest) and a poster.
- **The demo is one recording** with the README's gif: `demo.tape` writes both
  `doc/demo.gif` and `temp/site/demo.mp4`.
- **Chapters** come from `# chapter: label` lines in the tape.
  `site/lib/tape.mjs` adds up what vhs spends before each one, then scales the
  estimates to the recording's real length, so the drift (vhs adds a little per
  command) is spread out rather than piling up at the end.
- **The tapes for the loops** are `site/tapes/sessions.tape` and `copy.tape`.
- **Two stills need a real kitty**: an image in a pane (vhs draws with xterm.js,
  which has no kitty graphics) and the mobile view. `captures.sh kitty` says how.
  Until they exist, their panels show no picture, and the build says so.

## Fonts

Self-hosted woff2 subsets in `site/static/fonts/`, committed and made by
`site/scripts/fonts.sh`, about 69 KB together:

- **JetBrains Mono** from JetBrains' own v2.304 release (sha256-pinned), not
  fontsource: fontsource's copy drops `← → ❯ ⋯`, which the page sets in mono.
- **IBM Plex Sans** from `@fontsource-variable/ibm-plex-sans`, weights 400-600.
- **Shippori Mincho**, cut down to the two characters it sets: 欄 and 間.

Run `fonts.sh` again only to change what they cover. It needs fontTools and
brotli (`/usr/bin/python3`; mise's Python has neither).

## Tests

```sh
cd site
npm run lint      # Prettier
npm test          # the build's logic, node:test, at least 95% of lines and functions
npm run test:e2e  # builds, then Playwright at 1440 and on a 390-wide phone
```

- `e2e/frame.spec.mjs` is **generated** from the handoff by
  `~/.config/myconf/design-tools/gen-conformance.mjs` (map: `e2e/frame.map.json`)
  and pins every grid and flex frame the design declares. Its numbers are the
  design's hand-set values; don't tidy them onto a scale, and regenerate rather
  than edit it.
- `e2e/page.spec.mjs` covers what the frame can't: the page without JavaScript
  (every command as text, no copy button or theme toggle that would do nothing),
  the theme toggle and its memory, a copy button, the workspace pill following
  the section, no horizontal scroll, and no request to another origin.

Prettier leaves the markup in `lib/page.mjs` alone
(`embeddedLanguageFormatting: off`): it formats an `html`-tagged template as
HTML, which reflowed the page and its `<pre>` blocks the first time it ran.

## Deploying

[`.github/workflows/site.yml`](../.github/workflows/site.yml) runs the three
checks above on every pull request that touches the site, and on `main` and on
a `v*` tag it then deploys:

- **What triggers it**: a push to `main` touching `site/`,
  `doc/examples/screenshot/`, `scripts/demo/demo.tape` or the workflow; a release
  tag (the page shows the latest release); or a run by hand.
- **How**: `rsync --delete-delay --delay-updates` of `site/build/` to the VPS,
  so the new files land together at the end and an old page is never served
  half-replaced.
- **The key** (`SITE_DEPLOY_KEY`, a GitHub Actions secret) is a deploy-only
  ed25519 key. On the VPS it sits in `~/.ssh/authorized_keys` forced through
  `rrsync` and rooted at `~/sites/ranma`, with `restrict`: it can write there and
  nothing else, no shell, no forwarding. If it leaks, the damage is the
  website's contents.
- **The destination is named**, `public/`, never a bare `host:`. Through rrsync
  the two mean the same folder, but without the lock a bare `host:` is the home
  directory, and an `rsync --delete` aimed at one deleted most of the VPS home
  on 2026-11-03 (Masshiro card c60). Named, the worst a broken lock can do is
  write `~/public`.
- **The host key** is pinned (`SITE_KNOWN_HOSTS`), never accepted on first sight.
- **On the VPS** Caddy serves `~/sites/ranma/public` (`~/sites` is mounted
  read-only at `/srv/sites`) for `ranma.masshiro.pro`, from the homelab repo's
  `caddy/`: hashed assets `immutable` for a year, the page `no-cache`, a strict
  CSP. The hub lists it from `sites.yaml`, and the `blackbox-public` probe watches
  it.

Wildcard DNS already points `*.masshiro.pro` at the VPS, so there is no record
to add.

## Where it departs from the handoff

The handoff is [`handoffs/website/Main.dc.html`](handoffs/website/Main.dc.html)
(desktop, dark) and `Phone.dc.html` (390, light). Each departure, and why:

- **The tuios panel says nothing against tuios.** The design had "ranma aims to
  be the faster, less buggy daily driver". The owner, 2026-11-03: "dont say tuios
  is buggy, seems like we are throwing shade". It now credits tuios as the model
  for the feel and lists ranma's own trade-offs. `page.spec.mjs` holds this; the
  internal comparison in DESIGN.md's "Why not tuios" stays where it is and is not
  copy for the site.
- **Chapter four is "leader 2 · another workspace"**, not "alt+2": the tape
  presses the leader, and the label says what the video shows.
- **The copy-mode loop ends at the yank.** The design drew a paste on the local
  side of an ssh pane; vhs runs xterm.js with no system clipboard and no ssh
  server in the staged home, so a recording could not show it honestly. The
  panel's text still says what OSC 52 does.
- **Captures not yet recorded show no picture**, rather than the design's
  dashed placeholder with its "capture · loop · 1200×675" caption, which was a
  note to the builder, not page content.
- **The workspace pills mark the section on screen** only with JavaScript; the
  design drew "1 demo" as always current, which is wrong once you scroll.
- **Without JavaScript the theme toggle and copy buttons are not shown**: the
  handoff's note asked for that, and a control that does nothing is worse than
  none.
- **The videos don't autoplay for a visitor who asked for reduced motion**, and
  play only while on screen.

## What it doesn't do yet

- The two kitty stills (an image in a pane, the mobile view) and the videos need
  recording: `captures.sh videos` needs vhs and ttyd installed, and the stills
  are taken by hand. Until then the hero shows the README screenshot.
- No rendered docs. The man pages and CONFIG.md are linked on GitHub; the
  handoff suggested rendering them at build time as a later step.
- One language, English.
