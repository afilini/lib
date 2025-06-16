{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.portal-rest;
in {
  options.services.portal-rest = {
    enable = mkEnableOption "Portal REST API server";

    package = mkOption {
      type = types.package;
      description = "The portal-rest package to use.";
    };

    port = mkOption {
      type = types.port;
      default = 8000;
      description = "Port to listen on";
    };

    authToken = mkOption {
      type = types.str;
      description = "Authentication token for Portal SDK";
    };

    nostrKey = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Nostr private key (nsec format). Mutually exclusive with nostrKeyFile.";
    };

    nostrKeyFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Path to file containing the Nostr private key (nsec format). Mutually exclusive with nostrKey.";
    };

    nostrSubkeyProof = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Optional Nostr subkey proof in JSON format.";
    };

    nostrRelays = mkOption {
      type = types.listOf types.str;
      default = [ "wss://relay.nostr.net" "wss://relay.damus.io" ];
      description = "List of Nostr relays to connect to";
    };

    nwcUrl = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Optional Nostr Wallet Connect URL";
    };

    stateDir = mkOption {
      type = types.path;
      default = "/var/lib/portal-rest";
      description = "Directory to store state files";
    };

    host = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "Host to bind the server to";
    };
  };

  config = let
    # Validate that exactly one of nostrKey or nostrKeyFile is set
    hasKey = cfg.nostrKey != null;
    hasKeyFile = cfg.nostrKeyFile != null;
    keyConfig = if hasKey && hasKeyFile then
      throw "Cannot set both services.portal-rest.nostrKey and services.portal-rest.nostrKeyFile"
    else if !hasKey && !hasKeyFile then
      throw "Must set either services.portal-rest.nostrKey or services.portal-rest.nostrKeyFile"
    else if hasKeyFile then
      { NOSTR_KEY_FILE = cfg.nostrKeyFile; }
    else
      { NOSTR_KEY = cfg.nostrKey; };

    # Combine all environment variables
    envConfig = {
      PORT = toString cfg.port;
      AUTH_TOKEN = cfg.authToken;
      NOSTR_RELAYS = concatStringsSep "," cfg.nostrRelays;
    } // keyConfig // (if cfg.nostrSubkeyProof != null then {
      NOSTR_SUBKEY_PROOF = cfg.nostrSubkeyProof;
    } else {}) // (if cfg.nwcUrl != null then {
      NWC_URL = cfg.nwcUrl;
    } else {});

    # Only include the key file in read-only paths
    readOnlyPaths = if hasKeyFile then [ cfg.nostrKeyFile ] else [];
  in mkIf cfg.enable {
    assertions = [
      {
        assertion = (cfg.nostrKey != null) != (cfg.nostrKeyFile != null);
        message = "Either nostrKey or nostrKeyFile must be set, but not both";
      }
    ];

    systemd.services.portal-rest = {
      description = "Portal REST API server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      environment = envConfig;

      serviceConfig = {
        ExecStart = "${cfg.package}/bin/rest";
        Restart = "always";
        DynamicUser = true;
        StateDirectory = "portal-rest";
        StateDirectoryMode = "0750";
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        NoNewPrivileges = true;
        ReadOnlyPaths = readOnlyPaths;
      };
    };

    users.users.portal-rest = {
      isSystemUser = true;
      group = "portal-rest";
      home = cfg.stateDir;
      createHome = true;
    };

    users.groups.portal-rest = {};
  };
} 