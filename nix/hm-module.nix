# Home-manager module for Yad speech-to-text
#
# Provides a systemd user service for autostart.
# Usage: imports = [ yad.homeManagerModules.default ];
#        services.yad.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.yad;
in
{
  options.services.yad = {
    enable = lib.mkEnableOption "Yad speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "yad.packages.\${system}.yad";
      description = "The Yad package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.yad = {
      Unit = {
        Description = "Yad speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/yad";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
