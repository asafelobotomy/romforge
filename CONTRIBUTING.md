# Contributing to romforge

1. Fork the repository and create a branch from `main`.
2. Make your change and add or update tests.
3. Run what CI runs (house rule 2 in AGENTS.md): `cargo fmt --all --check`,
   `cargo clippy --all-targets -- -D warnings` and `cargo test`. Rust 1.93
   or newer; the app's build is in GUIDE.md.
4. Open a pull request describing what changed and why.
