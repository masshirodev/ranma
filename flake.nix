# Nix: `nix run github:masshirodev/ranma`, or the package in a flake or a
# NixOS/home-manager config (packages.<system>.default, or the overlay).
# Built from source with nixpkgs' Rust; doc/RELEASING.md says how to update it.
{
  description = "A tiling window manager for the terminal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
      ranma =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "ranma";
          inherit version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.installShellFiles ];
          # The flake's source has no .git: build.rs is told the commit, and
          # that Nix built it, so it never tries to update itself from source.
          RANMA_SOURCE_SHA = self.rev or self.dirtyRev or "";
          RANMA_PACKAGED = "Nix (update the flake input, or nix profile upgrade)";
          # The suite needs a shell, PTYs and a writable HOME; CI runs it.
          doCheck = false;
          postInstall = ''
            installManPage doc/man/man1/*.1 doc/man/man5/*.5 doc/man/man7/*.7
          '';
          meta = {
            description = "A tiling window manager for the terminal: i3's container tree and Hyprland's dwindle placement, where every window is a PTY";
            homepage = "https://github.com/masshirodev/ranma";
            license = with pkgs.lib.licenses; [ mit asl20 ];
            platforms = pkgs.lib.platforms.linux;
            mainProgram = "ranma";
          };
        };
    in
    {
      packages = forAll (pkgs: {
        default = ranma pkgs;
        ranma = ranma pkgs;
      });
      overlays.default = final: _prev: { ranma = ranma final; };
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer tmux ];
        };
      });
    };
}
