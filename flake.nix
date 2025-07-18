{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix.url = "github:nix-community/fenix";
  };

  outputs =
    {
      self,
      flake-utils,
      naersk,
      nixpkgs,
      fenix,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
          overlays = [ fenix.overlays.default ];
        };
        rust-toolchain = (
          pkgs.fenix.stable.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rustc"
            "rustfmt"
          ]
        );
        naersk' = pkgs.callPackage naersk {
          rustc = rust-toolchain;
          cargo = rust-toolchain;
        };
      in
      {
        packages = rec {
          portal = naersk'.buildPackage {
            src = ./.;
          };
          default = portal;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rust-analyzer
            lldb
            rust-toolchain
          ];
        };
      }
    );
}
