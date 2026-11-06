default: test

test: test-rust typecheck test-ui

test-rust:
    cargo test

typecheck:
    cd ui && bun run typecheck

test-ui:
    cd ui && bun test
