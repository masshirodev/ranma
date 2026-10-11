# The demo gif

`doc/demo.gif`, under the README's introduction, is recorded with
[vhs](https://github.com/charmbracelet/vhs), not captured by hand. Re-record it
when the screens it shows change:

```sh
scripts/demo/record.sh
```

- `stage.sh` builds a world of its own under a scratch directory: a home with
  a plain bash prompt and a pane title that is only the directory (the
  distribution's bashrc titles a pane `user@host:dir`, and the bar shows
  titles), btop showing only its CPU box (a process list would record the
  machine's own processes, and the CPU box alone fits the smallest layout), a
  ranma config with no plugin manager (hako would clone itself mid-recording),
  and a shallow clone of this repository to work in.
- `record.sh` runs `demo.tape` with that home, its own `PATH` and its own
  `XDG_RUNTIME_DIR`, so the ranma it starts is a private server that can see
  none of the machine's sessions, and ends that server afterwards. It uses the
  `ranma` on `PATH`, or `RANMA_BIN`. Never run `vhs demo.tape` directly: it
  would record your own home and start a server beside your real ones.
- `demo.tape` is the script: nvim, btop and a git log tiled by dwindle,
  `leader space` through the layouts, a float, a second workspace and back, a
  toast from `ranma notify`, and the which-key hint.

**WM mode and `Escape`.** A key that opens a pane ends WM mode; layouts,
focus, float and workspace switches keep it until `esc`. The tape presses
`Escape` only where the mode is still on: one sent to a shell reaches readline,
which reads it and the next letter as one `alt+` key, and the letter is lost
(`btop` becomes `top`).

It needs vhs and what vhs needs (ttyd, ffmpeg, a Chromium), plus btop, nvim and
git. On Arch, `pacman -S vhs`; without root, `go install
github.com/charmbracelet/vhs@v0.10.0` and ttyd's static release binary on
`PATH` work the same. Set `DEMO_DIR` to keep the staged world for a look
afterwards.
