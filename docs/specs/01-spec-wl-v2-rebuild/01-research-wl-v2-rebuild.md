# 01-research-wl-v2-rebuild.md

Current-standards research (July 2026, verified against crates.io/docs.rs/ratatui.rs
by a research agent). Implementation agents should follow this, not training data.

## Pinned crate versions

| Crate | Version | Notes |
|---|---|---|
| ratatui | 0.30 (0.30.2) | re-exports crossterm at root — use the re-export |
| crossterm | 0.29 | via ratatui re-export; do not pin separately unless needed |
| clap | 4.6 | derive feature |
| serde | 1.0 | derive |
| serde_json | 1.0 | |
| chrono | 0.4 | serde feature for RFC3339 |
| serde_norway | 0.9 | drop-in serde_yaml replacement (serde_yaml + serde_yml are BOTH dead) |
| color-eyre | 0.6 | idiomatic for ratatui apps; install hook in main |

## Ratatui 0.30 idioms

- `ratatui::init()` / `ratatui::restore()` exist; `init()` enables raw mode + alt
  screen + installs a panic hook, returns `DefaultTerminal`.
- Preferred entry: `ratatui::run(|terminal| App::default().run(terminal))` —
  init → run → restore even on error.
- Event loop: synchronous `crossterm::event::read()` (or `poll` for tick rate),
  state struct + view enum, `handle_key` dispatch. No async needed for this app.

## $EDITOR suspend/resume (lazygit pattern)

1. `ratatui::restore()` (leaves alt screen, disables raw mode, shows cursor)
2. `std::process::Command::new(editor).arg(path).status()?` — inherits stdio by
   default; block on status. Re-init even if the child fails (guard).
3. `ratatui::init()` again, then `terminal.clear()?` before next draw (ghosting).
- Note: `Terminal::suspend()/resume()` are for Ctrl-Z job control, NOT for this.

## Testing

- `TestBackend`: `Terminal::new(TestBackend::new(w, h))`, draw, then
  `terminal.backend().assert_buffer_lines(...)` / compare Buffer. Feed synthetic
  `KeyEvent`s into `handle_key` for state-transition coverage.

## Homebrew

- Chosen path: tag-triggered GitHub Actions release building
  `aarch64-apple-darwin` tarball + shasum; hand-maintained formula in a
  `sdavisde/homebrew-tap` repo (`Formula/wl.rb`), template kept in this repo.
  `brew install sdavisde/tap/wl`.
- Upgrade path (optional, documented in README): `dist` (formerly cargo-dist,
  v0.32, maintained) auto-generates the workflow AND pushes the formula to the
  tap on every tag.

Minimal formula shape:

```ruby
class Wl < Formula
  desc "Keyboard-first worklog TUI"
  homepage "https://github.com/sdavisde/worklog"
  version "X.Y.Z"
  on_macos do
    on_arm do
      url "https://github.com/sdavisde/worklog/releases/download/vX.Y.Z/wl-aarch64-apple-darwin.tar.gz"
      sha256 "..."
    end
  end
  def install
    bin.install "wl"
  end
  test do
    system "#{bin}/wl", "--version"
  end
end
```
