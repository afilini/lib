# portal-app-lib

React Native bindings for the Portal App library

## Building

The recommended way to build the library is to use the provided `flake.nix` which includes all the required depdendencies. You can load the flake by using `direnv` or manually running `nix develop` within this directory.

Then install the node deps with `yarn`. Once that's done you can build the library by running `yarn ubrn:android` and `yarn ubrn:ios`.