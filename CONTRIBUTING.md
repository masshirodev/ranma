# Contributing to ranma

Contributions are welcome. Most of what people want to add to a terminal
window manager does not belong in this repository, though. ranma keeps a small
core and puts everything else in plugins. So the first thing to settle is
whether your idea goes in the core or in a plugin.

## Core or plugin?

**If it can be built with the API that already exists, it is a plugin**, and it
lives in a repository of its own. That API is the same Lua that `init.lua`
uses: binds, actions, options, events (`ranma.on`), timers, reading a pane,
badges, screens and a plugin's own store. `doc/CONFIG.md`, "Plugins", is the
reference, and `ranma --dump-types` prints it for your editor. Name the
repository `<name>.ranma` and it installs with
[hako](https://github.com/masshirodev/hako.ranma), which ships with ranma.

**The core takes the primitives a plugin cannot do without**: an event that
does not exist yet, a value a plugin cannot read, a block a screen cannot
draw, an action the socket does not expose. A primitive should be general,
something several plugins could use, and not one plugin's feature moved into
Rust. The core also takes fixes, layouts and behaviour that `doc/DESIGN.md`
already counts as part of the window manager.

Some examples:

| Idea                                           | Where it goes                                                                    |
| ---------------------------------------------- | -------------------------------------------------------------------------------- |
| Search scrollback and jump to a match          | A plugin ([history.ranma](https://github.com/masshirodev/history.ranma))         |
| Ctrl+click a path to open it                   | A plugin ([openpath.ranma](https://github.com/masshirodev/openpath.ranma))       |
| Show what an AI agent in each pane is doing    | A plugin ([agents.ranma](https://github.com/masshirodev/agents.ranma)). AI never goes in the core |
| Run a command when a directory is entered      | A plugin                                                                         |
| Process a pane's output as it arrives          | An external program driving ranma through the socket (`ranma action`, `send`, `capture`, `wait`, `popup`, `ranma lua`) |
| "My plugin needs an event when X happens"      | The core: a new event                                                            |
| A pane loses scrollback after a reattach       | The core: a bug                                                                  |
| A new tiling layout                            | The core, but open an issue first                                                |

These are never accepted, however useful they would be:

- Lua that runs on the render or PTY path, so nothing per frame or per byte.
  A plugin's drawing is a plain value that Rust draws.
- Native or WASM plugins. The two ways to extend ranma are Lua and the socket.
- Lenient config parsing. An unknown name is an error that names the offender.
- Anything on the non-goals list in `doc/DESIGN.md` (remote hosts, multi-client
  tree sync, AI in the core, tmux parity). The design has to change first,
  and that is an issue, not a pull request.

Not sure? Open an issue and ask before you write the code.

## Reporting a bug

Include what you did, what you expected, and what happened. Add `ranma --version`,
your terminal, and the output of `ranma health` if a plugin or the config is
involved. A minimal `init.lua` that reproduces it is the most useful thing you
can send.

## Making a change

1. Read `doc/DESIGN.md` for the area you are touching. It records why things
   are the way they are.
2. Arm the git hooks once per clone with `scripts/hooks.sh`. Do not skip them.
3. Before a commit, run:

   ```sh
   cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
   scripts/smoke.sh    # when the change touches app, pane, render or input
   ```

   `doc/TESTING.md` says what each check covers.

4. Keep the docs in step, in the same commit:
   - A new setting, action or event goes in `assets/init.lua` (it **is** the
     default config), in `doc/CONFIG.md`, and in a test.
   - A decision changes `doc/DESIGN.md`.
   - Finished roadmap work ticks `doc/ROADMAP.md`.
5. Logic lives in the library, not in `src/main.rs`, so it can be tested
   without a terminal.

`AGENTS.md` has the full conventions.

### Commit messages

Write a sentence that says what changed and why, in the style of `git log`.
The `commit-msg` hook refuses `Co-Authored-By` trailers, `Claude-Session`
references and the robot emoji. If you used an AI tool, you are still the
author, so commit under your own name.

## License

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in ranma by you, as defined in the Apache-2.0 license, shall be
dual licensed under Apache-2.0 and MIT, without any additional terms or
conditions.
