{
  lib,
  stdenvNoCC,
  rustPlatform,
  bun,
  nodejs,
  pkg-config,
  wrapGAppsHook3,
  copyDesktopItems,
  makeDesktopItem,
  git,
  gh,
  openssl,
  gtk3,
  webkitgtk_4_1,
  libsoup_3,
  glib-networking,
  librsvg,
  cairo,
  gdk-pixbuf,
  pango,
  at-spi2-atk,
  atkmm,
  harfbuzz,
}:

let
  fs = lib.fileset;
  version = (lib.importTOML ../src-tauri/Cargo.toml).package.version;

  # The frontend's dependencies. Fetching needs the network, so this is a fixed-output derivation:
  # when ui/bun.lock changes, set the hash to lib.fakeHash, build once, and paste the hash from
  # the error message.
  nodeModules = stdenvNoCC.mkDerivation {
    pname = "pr-review-ui-node-modules";
    inherit version;
    src = fs.toSource {
      root = ../ui;
      fileset = fs.unions [
        ../ui/package.json
        ../ui/bun.lock
      ];
    };
    nativeBuildInputs = [ bun ];
    dontConfigure = true;
    dontFixup = true;
    buildPhase = ''
      runHook preBuild
      export HOME=$TMPDIR
      bun install --frozen-lockfile --no-progress --ignore-scripts
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      cp -r node_modules $out
      runHook postInstall
    '';
    outputHashMode = "recursive";
    outputHashAlgo = "sha256";
    outputHash = "sha256-0W2oZi0VLDcNNwnFnVgE9TGa0WdPOzFg2f32j4GFIqI=";
  };

  # The built frontend (`ui/dist`), which Tauri embeds into the binary.
  ui = stdenvNoCC.mkDerivation {
    pname = "pr-review-ui";
    inherit version;
    src = fs.toSource {
      root = ../ui;
      # The fake PR the browser mode reads is not part of the app.
      fileset = fs.difference ../ui ../ui/public/pr;
    };
    nativeBuildInputs = [
      bun
      nodejs
    ];
    dontConfigure = true;
    dontFixup = true;
    buildPhase = ''
      runHook preBuild
      cp -r ${nodeModules} node_modules
      chmod -R u+w node_modules
      # The tools' `#!/usr/bin/env node` does not exist in the sandbox.
      patchShebangs node_modules
      bun run build
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      cp -r dist $out
      runHook postInstall
    '';
  };
in
rustPlatform.buildRustPackage {
  pname = "pr-review";
  inherit version;

  src = fs.toSource {
    root = ../.;
    fileset = fs.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../backend
      ../src-tauri
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [
    "-p"
    "pr-review-app"
  ];
  # What `cargo tauri build` adds: without it the binary loads the dev server URL instead of the
  # embedded frontend.
  buildFeatures = [ "tauri/custom-protocol" ];
  doCheck = false;

  postPatch = ''
    mkdir -p ui
    cp -r ${ui} ui/dist
  '';

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook3
    copyDesktopItems
  ];
  buildInputs = [
    openssl
    gtk3
    webkitgtk_4_1
    libsoup_3
    glib-networking
    librsvg
    cairo
    gdk-pixbuf
    pango
    at-spi2-atk
    atkmm
    harfbuzz
  ];

  # Not shown in menus: it only exists so Wayland compositors find the icon through the app id
  # (see "Installing and the window icon" in CLAUDE.md). Running it without a PR number only prints
  # the usage message.
  desktopItems = [
    (makeDesktopItem {
      name = "pr-review";
      desktopName = "pr-review";
      comment = "Review GitHub pull requests";
      exec = "pr-review";
      icon = "pr-review";
      categories = [ "Development" ];
      startupWMClass = "com.cognite.pr-review";
      noDisplay = true;
    })
  ];

  postInstall = ''
    rm -rf $out/lib
    for size in 32x32 64x64 128x128; do
      install -Dm644 src-tauri/icons/$size.png $out/share/icons/hicolor/$size/apps/pr-review.png
    done
    install -Dm644 "src-tauri/icons/128x128@2x.png" $out/share/icons/hicolor/256x256/apps/pr-review.png
  '';

  # pr-review runs git and gh; use the ones on your PATH when there are any.
  preFixup = ''
    gappsWrapperArgs+=(--suffix PATH : ${
      lib.makeBinPath [
        git
        gh
      ]
    })
  '';

  meta = {
    description = "Desktop app for reviewing GitHub pull requests";
    mainProgram = "pr-review";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
