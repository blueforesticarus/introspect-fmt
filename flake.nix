{
  description = "Development environment for the introspect-fmt Rust workspace";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      # Match platforms supported by this nixpkgs revision (Intel macOS was dropped).
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            # Use a single pinned nixpkgs toolchain for builds, linting, and editing.
            packages = with pkgs; [ cargo rustc rustfmt clippy rust-analyzer ];
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            # Host linker overrides may reference tools outside this shell.
            # Developers can still set custom flags after entering the shell.
            RUSTFLAGS = "";
          };
        });
    };
}
