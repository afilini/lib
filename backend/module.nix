{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.portal-backend;
in
{
  options.services.portal-backend = {
    enable = mkEnableOption "Portal Backend server";

    package = mkOption {
      type = types.package;
      default = pkgs.portal-backend;
      defaultText = literalExpression "pkgs.portal-backend";
      description = "The portal-backend package to use.";
    };

    host = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "Host to bind the server to";
    };

    port = mkOption {
      type = types.port;
      default = 3001;
      description = "Port to bind the server to";
    };

    databasePath = mkOption {
      type = types.str;
      default = "/var/lib/portal-backend/sessions.db";
      description = "Path to the SQLite database file";
    };

    restApiUrl = mkOption {
      type = types.str;
      description = "URL of the Portal REST API server";
      example = "http://localhost:3000";
    };

    authToken = mkOption {
      type = types.str;
      description = "Authentication token for the REST API";
    };
  };

  config = mkIf cfg.enable {
    systemd.services.portal-backend = {
      description = "Portal Backend server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      serviceConfig = {
        Type = "simple";
        User = "portal-backend";
        Group = "portal-backend";
        Restart = "on-failure";
        RestartSec = "5s";
        ExecStart = "${cfg.package}/bin/portal-backend";
        WorkingDirectory = "/var/lib/portal-backend";
        StateDirectory = "portal-backend";
        
        # Security settings
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectHostname = true;
        ProtectClock = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
        RestrictNamespaces = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        RemoveIPC = true;
        CapabilityBoundingSet = "";
      };

      environment = {
        HOST = cfg.host;
        PORT = toString cfg.port;
        DATABASE_PATH = cfg.databasePath;
        REST_API_URL = cfg.restApiUrl;
        AUTH_TOKEN = cfg.authToken;
      };
    };

    users.users.portal-backend = {
      description = "Portal Backend service user";
      isSystemUser = true;
      group = "portal-backend";
    };

    users.groups.portal-backend = {};
  };
} 