# ranma

A tiling window manager for the terminal: i3's container tree and Hyprland's
dwindle placement, where every window is a PTY. Linux only (x64 and arm64,
glibc 2.34 or newer).

```sh
npm install -g @masshirodev/ranma
ranma
```

This package runs the release binary from `@masshirodev/ranma-linux-x64` or
`-arm64`, which npm picks for your machine. Node stays running beside each
terminal's ranma client. A package manager or the install script avoids that:
see [Install](https://github.com/masshirodev/ranma#install).

After an upgrade, run `ranma --check-config && ranma upgrade --all` to move
running servers onto the new build.

Documentation, configuration and source: https://github.com/masshirodev/ranma
