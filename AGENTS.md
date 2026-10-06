# AGENTS.md

ranma is a Rust tiling window manager for the terminal. Read `doc/DESIGN.md`
before changing behaviour: it records the decisions and, more importantly, the
non-goals. A feature on the non-goals list needs the design changed first, not a PR.

## Before a commit

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
scripts/smoke.sh    # when the change touches app, pane, render or input
```

To install what you built: `./install.sh` (it checks the new binary against the
user's config and restores the previous one if it is rejected). `./install.sh
--check` runs the gate above first. Prefer it to a bare `cargo install`.

The git hooks in `.githooks/` enforce this: fmt and clippy on commit, the tests
and the smoke on push, and `commit-msg` refuses `Co-Authored-By` trailers,
`Claude-Session` references and the robot emoji. A fresh clone runs none of them
until `scripts/hooks.sh` has armed it once. Make them pass; do not set skip flags.

`doc/TESTING.md` says what each check covers.

## Conventions

- Push to `main`.
- The binary (`src/main.rs`) is a thin CLI; logic lives in the library so it can be
  tested without a terminal.
- `assets/init.lua` **is** the default configuration and the reference users get from
  `--dump-config`. A new setting, action or event goes there, in `doc/CONFIG.md`, and
  in a test in the same change.
- Config input is parsed strictly: unknown names are errors naming the offender.
  Never add a lenient fallback that ignores what it does not understand.
- Lua never runs on the render or PTY path: only binds, hooks, and throttled ticks.
- Decisions change `doc/DESIGN.md` in the same commit; finished work ticks
  `doc/ROADMAP.md`.
- **A new theme key goes into a user's theme only after the binary that knows
  it is installed.** Theme keys are parsed strictly, so a theme file (or a
  template that renders one) gaining the key before `./install.sh` makes the
  installed ranma reject the theme, and a shell that starts ranma falls back to
  a plain shell. Order: add the key here with a default, `./install.sh`, then
  the theme.
- `AGENTS.local.md` (gitignored) carries what is true of one checkout on one
  machine. Read it when it exists; never commit it.
