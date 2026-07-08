# wl

A keyboard-first worklog TUI: instant task capture from the shell, an
always-on-top-free `ratatui` terminal UI for tasks and long-running notes, and
a permanent, AI-readable archive of everything you complete. See `DESIGN.md`
for the full design record.

## Install from source

```sh
cargo build --release
# binary at target/release/wl
```

## Usage

_Coming soon — filled in as Unit 2+ lands._

## Keybinds

_Coming soon — filled in once the TUI (Unit 4) lands._

## Homebrew

_Coming soon — a `sdavisde/homebrew-tap` formula will be published once
release packaging (Unit 5) lands._

## Development

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Enable the local pre-commit hook (runs the three checks above):

```sh
git config core.hooksPath .githooks
```
