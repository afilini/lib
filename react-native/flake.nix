{
  description = "Rust development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;

          config.android_sdk.accept_license = true;
          config.allowUnfree = true;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "aarch64-linux-android" "x86_64-linux-android" ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        android = {
          buildToolsVersion = "35.0.0";
          cmakeVersion = "3.22.1";
        };
        androidComposition = pkgs.androidenv.composeAndroidPackages {
          buildToolsVersions = [ android.buildToolsVersion ];
          platformVersions = [ "35" ];
          includeNDK = true;
          ndkVersion = "27.1.12297006";
          cmakeVersions = [ android.cmakeVersion ];
        };
      in
      {
        devShells = {
          default = pkgs.mkShell rec {
            buildInputs = with pkgs; [
              rustToolchain
              nodejs
              cmake
              ninja
              clang-tools
              cargo-ndk
              jdk
              yarn
            ];

            ANDROID_SDK_ROOT = "${androidComposition.androidsdk}/libexec/android-sdk";
            ANDROID_NDK_ROOT = "${ANDROID_SDK_ROOT}/ndk-bundle";

            # Ensures that we don't have to use a FHS env by using the nix store's aapt2.
            GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${ANDROID_SDK_ROOT}/build-tools/${android.buildToolsVersion}/aapt2";
          };
        };

        packages = pkgs.lib.optionalAttrs (pkgs.lib.elem system [ "x86_64-linux" "x86_64-darwin" ]) (
          let
            version = (builtins.fromJSON (builtins.readFile ./package.json)).version;
            filteredSrc = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                let
                  base = baseNameOf path;
                in
                  ! (builtins.elem base [ "node_modules" ]);
            };
            cargoDeps = rustPlatform.fetchCargoVendor {
              src = ../app;
              hash = "sha256-PPff8r7FYYW1cAZUVyd+0CxeWnffOwCvxaqnABjBfII=";
              patchPhase = ''
                cp ${../Cargo.lock} Cargo.lock
              '';
            };

            npm-package = pkgs.buildNpmPackage {
              inherit version cargoDeps;
              pname = "portal-app-lib-npm";

              src = filteredSrc;

              nativeBuildInputs = with pkgs; [
                rustToolchain
                nodejs
                cmake
                ninja
                clang-tools
                cargo-ndk
                jdk
              ];

              configurePhase = ''
                export HOME=$(mktemp -d)
                export ANDROID_SDK_ROOT="${androidComposition.androidsdk}/libexec/android-sdk"
                export ANDROID_NDK_ROOT="$ANDROID_SDK_ROOT/ndk-bundle"
                export GRADLE_OPTS="-Dorg.gradle.project.android.aapt2FromMavenOverride=$ANDROID_SDK_ROOT/build-tools/${android.buildToolsVersion}/aapt2"
              '';

              npmDepsHash = "sha256-4q1nZYafuglBVXjcPHBM1ZiIZSdCtSkivPeb2JmaIZI=";
              npmFlags = [ "--legacy-peer-deps" ];
              # dontNpmBuild = true;
              # npmBuildScript = "npm ";

              preBuild = ''
                npm run prepare
              '';

              buildPhase = ''
                npm run build
              '';

              postBuild = ''
                ls -la .
              '';
            };
          in
          {
            npm-package = npm-package;
            cargo-deps = cargoDeps;

            portal-app-lib = pkgs.stdenv.mkDerivation rec {
              pname = "portal-app-lib";
              version = (builtins.fromJSON (builtins.readFile ./package.json)).version;

              src = pkgs.lib.cleanSourceWith {
                src = ./.;
                filter = path: type:
                  let
                    base = baseNameOf path;
                  in
                    ! (builtins.elem base [ "node_modules" "example" ]);
              };

              nativeBuildInputs = with pkgs; [
                rustToolchain
                nodejs
                cmake
                ninja
                clang-tools
                cargo-ndk
                jdk
              ];

              configurePhase = ''
                export HOME=$(mktemp -d)
                export ANDROID_SDK_ROOT="${androidComposition.androidsdk}/libexec/android-sdk"
                export ANDROID_NDK_ROOT="$ANDROID_SDK_ROOT/ndk-bundle"
                export GRADLE_OPTS="-Dorg.gradle.project.android.aapt2FromMavenOverride=$ANDROID_SDK_ROOT/build-tools/${android.buildToolsVersion}/aapt2"
                
                # Use pre-built dependencies
                ln -s ${npm-package}/node_modules ./node_modules
              '';

              buildPhase = ''
                echo "Building Android release..."
                npm run ubrn:android -- --release

                echo "Applying uniffi patches..."
                node ./patch-uniffi-bindgen.js

                echo "Creating npm package..."
                npm pack
              '';

              installPhase = ''
                mkdir -p $out
                cp *.tgz $out/
                
                # Also copy the built library files for easier access
                mkdir -p $out/lib
                cp -r lib/* $out/lib/ 2>/dev/null || true
                cp -r android $out/ 2>/dev/null || true
                cp -r ios $out/ 2>/dev/null || true
                cp -r src $out/ 2>/dev/null || true
                cp package.json $out/
              '';

              meta = with pkgs.lib; {
                description = "React Native bindings for the Portal App library";
                homepage = "https://github.com/ProjectPortal/lib";
                platforms = [ "x86_64-linux" "x86_64-darwin" ];
                maintainers = [ ];
              };
            };
          }
        );
      }
    );
}
