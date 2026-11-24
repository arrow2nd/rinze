self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.rinzed;
  inherit (pkgs.stdenv.hostPlatform) isDarwin isLinux system;
in
{
  options.services.rinzed = {
    enable = lib.mkEnableOption "rinzed をログイン時に起動し、フロントエンドが同じ版の rinzed を共有する";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${system}.rinzed;
      defaultText = lib.literalExpression "rinze.packages.\${system}.rinzed";
      description = "常駐させる rinzed。PATH にも入れるので、サービスが止まっているときにフロントエンドが起動するのも同じ版になる。";
    };
    macos = {
      enable = lib.mkEnableOption "macOS の入力メソッド (rinze.app) を ~/Library/Input Methods に入れる";
      package = lib.mkOption {
        type = lib.types.package;
        default = self.packages.${system}.rinze-macos.override { rinzed = cfg.package; };
        defaultText = lib.literalExpression "rinze.packages.\${system}.rinze-macos.override { rinzed = cfg.package; }";
        description = "入れる rinze.app。サービスが止まっているときは、これが `services.rinzed.package` の rinzed を起動する。";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.macos.enable -> isDarwin;
        message = "services.rinzed.macos.enable は macOS でだけ使える";
      }
    ];

    home.packages = [ cfg.package ];

    systemd.user.services.rinzed = lib.mkIf isLinux {
      Unit.Description = "rinze の常駐プロセス";
      Service = {
        # サービスの環境には利用者のシェルの XDG_DATA_HOME と XDG_STATE_HOME が入らないので、
        # フロントエンドが起動した rinzed と同じユーザー辞書と学習の履歴を引数で渡す
        ExecStart = lib.escapeShellArgs [
          (lib.getExe cfg.package)
          "--user-dict"
          "${config.xdg.dataHome}/rinze/user.jisyo"
          "--history"
          "${config.xdg.stateHome}/rinze/history.tsv"
        ];
        Restart = "on-failure";
        # rinzed は変換のたびに読みと候補を標準エラー出力へ書く。journal に残さない
        StandardError = "null";
      };
      Install.WantedBy = [ "default.target" ];
    };

    launchd.agents.rinzed = lib.mkIf isDarwin {
      enable = true;
      config = {
        ProgramArguments = [ (lib.getExe cfg.package) ];
        RunAtLoad = true;
        # ソケットを別の rinzed が使っていれば 0 で終了する。そのときは起動し直さない
        KeepAlive.SuccessfulExit = false;
        ProcessType = "Interactive";
      };
    };

    # macOS はシンボリックリンクの入力メソッドを読まないので、複製して置く。
    # 動いているアプリを上書きすると署名が合わなくなって起動できないので、消してから複製し、署名し直す
    home.activation.rinzeMacos = lib.mkIf cfg.macos.enable (
      lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        rinzeSource="${cfg.macos.package}/Library/Input Methods/rinze.app"
        rinzeApp="$HOME/Library/Input Methods/rinze.app"
        rinzeStamp="$HOME/Library/Input Methods/.rinze.app.source"
        if [ ! -d "$rinzeApp" ] || [ "$(cat "$rinzeStamp" 2>/dev/null)" != "$rinzeSource" ]; then
          run mkdir -p "$HOME/Library/Input Methods"
          run rm -rf "$rinzeApp"
          run cp -R "$rinzeSource" "$rinzeApp"
          run chmod -R u+w "$rinzeApp"
          run /usr/bin/codesign --force --sign - "$rinzeApp"
          run /usr/bin/pkill -x rinze || true
          run "$rinzeApp/Contents/MacOS/rinze" --register
          run sh -c 'printf %s "$1" > "$2"' sh "$rinzeSource" "$rinzeStamp"
        fi
      ''
    );
  };
}
