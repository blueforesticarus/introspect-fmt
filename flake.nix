{
  description = "Development environment for the introspect-fmt Rust workspace";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.fenix = {
    url = "github:nix-community/fenix";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, fenix }:
    let
      # Match platforms supported by this nixpkgs revision (Intel macOS was dropped).
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          # flake.lock pins Fenix and therefore the complete nightly's date.
          mkRustShell = rust: pkgs.mkShell {
            # Keep the compiler, tools, and standard-library sources in sync.
            packages = [ (rust.withComponents [
              "cargo" "rustc" "rust-src" "rustfmt" "clippy" "rust-analyzer"
            ]) ];
            RUST_SRC_PATH = "${rust.rust-src}/lib/rustlib/src/rust/library";
            # Host linker overrides may reference tools outside this shell.
            # Developers can still set custom flags after entering the shell.
            RUSTFLAGS = "";
          };
        in {
          default = mkRustShell fenix.packages.${system}.complete;
          # Exercise the fallback on a real stable compiler, not just feature-off nightly.
          stable = mkRustShell fenix.packages.${system}.stable;
        });
    };
}
