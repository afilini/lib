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
        rustToolchainApple = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "aarch64-apple-ios" "aarch64-apple-ios-sim" ];
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

        xcodeenv = import "${nixpkgs}/pkgs/development/mobile/xcodeenv" { inherit (pkgs) callPackage; };
        xcodewrapper = (xcodeenv.composeXcodeWrapper {
          versions = [ ];
          xcodeBaseDir = "/Applications/Xcode.app";
        });
      in
      {
        devShells.ios = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchainApple
            nodejs
            yarn
          ];

          shellHook = ''
            # This is set somewhere by nix
            unset DEVELOPER_DIR
            # We want to use stuff from the xcode wrapper over nixpkgs
            export PATH=${xcodewrapper}/bin:$PATH
          '';
        };
        devShells.default = pkgs.mkShell rec {
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
      }
    );
}
