# AGENTS.md

ranma is a Rust tiling window manager for the terminal. Read `doc/DESIGN.md`
before changing behaviour: it records the decisions and, more importantly, the
non-goals. A feature on the non-goals list needs the design changed first, not a PR.

## Before a commit

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
scripts/smoke.sh    # when the change touches app, pane, render or input
```

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
