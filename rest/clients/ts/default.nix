{ lib, buildNpmPackage }:

let
  packageJson = lib.importJSON ./package.json;
in
buildNpmPackage {
  pname = packageJson.name;
  version = packageJson.version;
  src = ./.;
  npmDepsHash = "sha256-FMDlTvFqtjCk7lVmDnBuuWlNmQVri9cbcD3vK24Y+1k="; # Will need to be updated
  
  buildPhase = ''
    npm run build
  '';
  
  installPhase = ''
    mkdir -p $out
    cp -r dist package.json $out/
  '';

  meta = with lib; {
    description = packageJson.description;
    license = licenses.mit;
    maintainers = [ ];
  };
} 