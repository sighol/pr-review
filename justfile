default: test

test: test-rust typecheck test-ui

test-rust:
    cargo test

typecheck:
    cd ui && bun run typecheck

test-ui:
    cd ui && bun test

run-dev:
    cargo tauri dev -- -- 3

# Builds with Tauri's bundler and installs what it would put in the .deb: the binary in
# ~/.cargo/bin, and the desktop entry and icons in ~/.local/share (so Wayland shows the app's icon).
install:
    rm -rf target/release/bundle/deb
    cargo tauri build --bundles deb
    cp target/release/pr-review ~/.cargo/bin/
    mkdir -p ~/.local/share
    cp -r target/release/bundle/deb/*/data/usr/share/. ~/.local/share/
    -gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
