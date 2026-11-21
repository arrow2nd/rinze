{
  description = "rinze: 左文脈で候補を並べる SKK 方式の IME";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAll =
        f:
        nixpkgs.lib.genAttrs systems (
          system: f (nixpkgs.legacyPackages.${system}.extend self.overlays.default)
        );
      lintTools = pkgs: {
        lua = [
          pkgs.stylua
          pkgs.lua51Packages.luacheck
        ];
        rust = [
          pkgs.rustfmt
          pkgs.clippy
          pkgs.cargo-shear
        ];
      };
    in
    {
      overlays.default = import ./nix/overlay.nix;

      packages = forAll (pkgs: {
        default = pkgs.rinzed;
        inherit (pkgs)
          rinzed
          rinzed-unwrapped
          rinze-nvim
          rinze-zenz-model
          ;
        rinze-dictionaries = pkgs.symlinkJoin {
          name = "rinze-dictionaries";
          paths = pkgs.rinzed.dictionaries;
        };
        demo = pkgs.callPackage ./nix/demo.nix { };
      } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
        fcitx5-rinze = pkgs.callPackage ./nix/fcitx5.nix { };
      });

      apps = forAll (pkgs: {
        demo = {
          type = "app";
          program = nixpkgs.lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.demo;
          meta.description = "rinze と blink.cmp を設定した Neovim を、一時ディレクトリの状態で起動する";
        };
      });

      checks = forAll (pkgs: pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
        fcitx5-rinze = (pkgs.callPackage ./nix/fcitx5.nix { }).overrideAttrs { doCheck = true; };
      } // {
        inherit (pkgs) rinzed rinze-nvim;
        lint-lua =
          pkgs.runCommand "rinze-lint-lua"
            {
              nativeBuildInputs = (lintTools pkgs).lua;
              src = pkgs.lib.fileset.toSource {
                root = ./.;
                fileset = pkgs.lib.fileset.unions [
                  ./.stylua.toml
                  ./.luacheckrc
                  ./nvim
                  ./tests
                  ./nix/demo.lua
                ];
              };
            }
            ''
              bash $src/tests/lint.sh lua
              touch $out
            '';
        lint-rust = pkgs.rinzed-unwrapped.overrideAttrs (old: {
          pname = "rinze-lint-rust";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./clippy.toml
              ./crates
              ./tests/lint.sh
            ];
          };
          nativeBuildInputs = old.nativeBuildInputs ++ (lintTools pkgs).rust;
          buildPhase = "bash tests/lint.sh rust";
          doCheck = false;
          installPhase = "touch $out";
        });
        default-socket =
          pkgs.runCommand "rinze-default-socket"
            {
              nativeBuildInputs = [ pkgs.neovim ];
              src = pkgs.lib.fileset.toSource {
                root = ./.;
                fileset = pkgs.lib.fileset.unions [
                  ./nvim
                  ./tests/nvim/default_socket.lua
                  ./crates/rinze-socket/tests/fixtures/default_socket.json
                ];
              };
            }
            ''
              HOME=$TMPDIR nvim --headless --clean -l $src/tests/nvim/default_socket.lua
              touch $out
            '';
        security-nvim =
          pkgs.runCommand "rinze-security-nvim"
            {
              nativeBuildInputs = [ pkgs.neovim ];
              src = pkgs.lib.fileset.toSource {
                root = ./.;
                fileset = pkgs.lib.fileset.unions [
                  ./nvim
                  ./tests/nvim/security.lua
                ];
              };
            }
            ''
              HOME=$TMPDIR nvim --headless --clean -l $src/tests/nvim/security.lua
              touch $out
            '';
        default-cmd =
          let
            rinzedWritingPidToHome = pkgs.writeShellScriptBin "rinzed" ''
              echo $$ > "$HOME/rinzed.pid"
              exec ${pkgs.lib.getExe pkgs.rinzed} "$@"
            '';
          in
          pkgs.runCommand "rinze-default-cmd"
            {
              nativeBuildInputs = [
                pkgs.neovim
                rinzedWritingPidToHome
              ];
              src = pkgs.lib.fileset.toSource {
                root = ./.;
                fileset = pkgs.lib.fileset.unions [
                  ./nvim
                  ./tests/nvim/default_cmd.lua
                ];
              };
            }
            ''
              mkdir -p run data state
              trap '[ -f rinzed.pid ] && kill "$(cat rinzed.pid)"' EXIT
              HOME=$PWD XDG_RUNTIME_DIR=$PWD/run XDG_DATA_HOME=$PWD/data XDG_STATE_HOME=$PWD/state \
                nvim --headless --clean -l $src/tests/nvim/default_cmd.lua
              touch $out
            '';
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell (
          {
            nativeBuildInputs = [
              pkgs.cmake
              pkgs.pkg-config
              pkgs.rustPlatform.bindgenHook
              pkgs.cargo
              pkgs.rustc
              pkgs.neovim
            ]
            ++ (lintTools pkgs).lua
            ++ (lintTools pkgs).rust;
            RINZE_BLINK = "${pkgs.vimPlugins.blink-cmp}";
          }
          // pkgs.lib.optionalAttrs (pkgs.rinzed-unwrapped ? RUSTFLAGS) {
            inherit (pkgs.rinzed-unwrapped) RUSTFLAGS;
          }
        );
      });
    };
}
