{
  lib,
  stdenv,
  writeShellApplication,
  neovim,
  procps,
  vimPlugins,
  rinzed,
  rinze-nvim,
}:
writeShellApplication {
  name = "rinze-demo";
  runtimeInputs = [
    neovim
    rinzed
  ]
  ++ lib.optional stdenv.hostPlatform.isLinux procps;
  text = ''
    # nix develop の TMPDIR は長く、ソケットのパスの上限を超えうるので /tmp に置く
    work=$(mktemp -d /tmp/rinze-demo-XXXXXX)
    cleanup() {
      pkill -f -- "--socket $work/rinze.sock" || true
      rm -rf "$work"
    }
    trap cleanup EXIT
    for dir in config data state cache; do
      mkdir -p "$work/$dir"
    done
    export XDG_CONFIG_HOME=$work/config XDG_DATA_HOME=$work/data XDG_STATE_HOME=$work/state XDG_CACHE_HOME=$work/cache
    export RINZE_DEMO_WORK=$work
    nvim --clean --cmd 'set runtimepath^=${rinze-nvim},${vimPlugins.blink-cmp}' -c 'luafile ${./demo.lua}' "$@"
  '';
}
