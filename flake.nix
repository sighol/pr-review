{
  description = "pr-review: a desktop app for reviewing GitHub PRs (Tauri + Rust + React)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        pr-review = pkgs.callPackage ./nix/package.nix { };
        default = pr-review;
      });

      apps = forAll (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/pr-review";
        };
      });

      # For a NixOS or home-manager config: nixpkgs.overlays = [ pr-review.overlays.default ];
      # and then pkgs.pr-review.
      overlays.default = final: _prev: {
        pr-review = final.callPackage ./nix/package.nix { };
      };

      devShells = forAll (pkgs: {
        default =
          let
            libs = with pkgs; [
              at-spi2-atk
              atkmm
              cairo
              gdk-pixbuf
              glib
              gtk3
              harfbuzz
              librsvg
              libsoup_3
              openssl
              pango
              webkitgtk_4_1
            ];
          in
          pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              pkg-config
              cargo
              rustc
              clippy
              rustfmt
              cargo-tauri
              bun
              just
              git
            ];
            buildInputs = libs;

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libs;
            XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
            GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules/";
          };
      });
    };
}
