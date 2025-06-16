{ lib
, stdenv
, rustPlatform
, fetchFromGitHub
, pkg-config
, openssl
, runCommand
}:

let
  # Read version from Cargo.toml
  cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
  version = cargoToml.package.version;
in

rustPlatform.buildRustPackage rec {
  pname = "portal-rest";
  inherit version;

  src = ../.;

  # Use workspace Cargo.lock
  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  cargoBuildFlags = [ "--package rest" ];
  cargoTestFlags = [ "--package rest" ];

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    openssl
  ];

  meta = with lib; {
    description = "Portal REST API server";
    homepage = "https://github.com/PortalTechnologiesInc/lib";
    license = licenses.mit;
    maintainers = with maintainers; [ ];
    platforms = platforms.linux;
  };
} 