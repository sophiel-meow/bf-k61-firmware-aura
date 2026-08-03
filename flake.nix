{
  description = "AURA firmware dev environment for Baofeng UV-K6";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rust-toolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "thumbv6m-none-eabi" ];
          extensions = [ "llvm-tools" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          name = "aura-fw";

          buildInputs = [
            rust-toolchain
            pkgs.cargo-binutils
            (pkgs.python3.withPackages (ps: [
              ps.pyserial
              ps.numpy
            ]))
            pkgs.gnumake
            pkgs.direwolf
          ];

          shellHook = ''
            echo " AURA firmware dev environment"
            echo "   rustc : $(rustc --version 2>/dev/null || echo '...')"
            echo "   cargo : $(cargo --version 2>/dev/null || echo '...')"
            echo "   target: thumbv6m-none-eabi"
            if command -v python3 &>/dev/null; then
              echo "   python: $(python3 --version 2>/dev/null || echo '...')"
            fi
            echo ""
            echo "  Build:  cargo build --release"
            echo "  Bin:    cargo objcopy --release -- -O binary aura.bin"
            echo "  Size:   cargo size --release"
          '';
        };
      }
    );
}
