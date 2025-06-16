{ lib, buildNpmPackage, nodejs, portal-ts-client, jq }:

let
  packageJson = lib.importJSON ./package.json;
in
buildNpmPackage {
  pname = packageJson.name;
  version = packageJson.version;
  src = ./.;
  npmDepsHash = "sha256-Q97WXfJRF6RMLGubHwaYkoHfgjbCvH2d18TO3n1PI/o=";
  
  # Remove the local dependency from package.json before npm install
  prePatch = ''
    # Create a modified package.json without the local dependency
    ${lib.getExe jq} 'del(.dependencies["portal-sdk"])' package.json > package.json.tmp
    mv package.json.tmp package.json
  '';
  
  preBuild = ''
    npm install ${portal-ts-client}
  '';
  
  buildPhase = ''
    npm run build
  '';
  
  installPhase = ''
    mkdir -p $out/bin $out/lib/portal-backend
    cp -r dist package.json public node_modules $out/lib/portal-backend/
    
    # Create wrapper script
    cat > $out/bin/portal-backend << EOF
    #!/bin/sh
    cd $out/lib/portal-backend
    exec ${nodejs}/bin/node dist/index.js "\$@"
    EOF
    chmod +x $out/bin/portal-backend
  '';

  nativeBuildInputs = [ jq ];

  meta = with lib; {
    description = packageJson.description;
    license = licenses.mit;
    maintainers = [ ];
  };
} 