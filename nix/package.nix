{
  lib,
  stdenvNoCC,
  rustPlatform,
  bun,
  nodejs,
  pkg-config,
  wrapGAppsHook3,
  cargo-tauri,
  jq,
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
    outputHash = "sha256-mYaDVgXqVIuSWPD/vVHHfSFxXjYqN4IKOxRJhOcpyB0=";
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

  # `cargo-tauri.hook` runs `cargo tauri build` from here, the same as the `install` recipe in
  # the justfile.
  buildAndTestSubdir = "src-tauri";

  # `cargo tauri build` does not enable this on its own: without it the binary loads the dev
  # server URL instead of the embedded frontend.
  buildFeatures = [ "tauri/custom-protocol" ];
  doCheck = false;

  postPatch = ''
    mkdir -p ui
    cp -r ${ui} ui/dist

    # The sandboxed build is offline, so drop the command that would otherwise re-run
    # `bun run build` via Tauri's own build pipeline; `frontendDist` above already has it.
    ${lib.getExe jq} 'del(.build.beforeBuildCommand)' src-tauri/tauri.conf.json > src-tauri/tauri.conf.json.new
    mv src-tauri/tauri.conf.json.new src-tauri/tauri.conf.json
  '';

  nativeBuildInputs = [
    cargo-tauri.hook
    jq
    pkg-config
    wrapGAppsHook3
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

  # The desktop entry and icons come from the real `deb` bundle (`tauriInstallHook`, driven by
  # `src-tauri/tauri.conf.json`'s `bundle` config), the same as the justfile's `install` recipe.
  # The entry is NoDisplay (see "Installing and the window icon" in CLAUDE.md): it only exists so
  # Wayland compositors find the icon through the app id. Running it without a PR number only
  # prints the usage message.

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
