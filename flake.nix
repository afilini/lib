{
  description = "Portal SDK";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    let
      # Common arguments for all systems
      allSystems = flake-utils.lib.eachDefaultSystem (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" ];
          };

          # Import the rest package from its default.nix
          rest = pkgs.callPackage ./rest/default.nix { 
            rustPlatform = pkgs.makeRustPlatform {
              cargo = rustToolchain;
              rustc = rustToolchain;
            };
          };
        in
        {
          packages = {
            inherit rest;
            portal-ts-client = pkgs.callPackage ./rest/clients/ts/default.nix { };
            backend = pkgs.callPackage ./backend/default.nix {
              portal-ts-client = self.packages.${system}.portal-ts-client;
            };
          };

          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              rustToolchain
            ];
          };
        }
      );

      # NixOS modules
      nixosModules = {
        portal-rest = import ./rest/module.nix;
        portal-backend = import ./backend/module.nix;
      };
    in
    allSystems // {
      inherit nixosModules;
    };
}
