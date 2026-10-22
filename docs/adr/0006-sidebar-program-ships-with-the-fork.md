# The sidebar program ships with the fork, not with loom

ADR 0002 said the sidebar pane runs loom's program. The fork instead builds it as
`herdr-sidebar`, a Rust (ratatui) crate in this repository. It ships as release
assets beside `herdr` and is installed by the same tap formula and
`scripts/dogfood.sh`. loom still decides what the sidebar means: it writes the
agent annotations the sidebar shows, owns the `[sidebar]` config in the dotfiles,
and overrides actions such as close with its own commands. The program reads
only herdr's API, so it also works for agents and panes loom never touched. This
supersedes 0002 only where it names loom as the program's owner.

## Considered Options

- **A crate in the dotfiles under loom.** This matches 0002 literally, but only the
  primary mac has a Rust toolchain, so every other unix box would need rustup and
  a build step.
- **Its own repository with its own releases.** This adds a third release pipeline
  and loses compile-time checks against herdr's API schema types.
- **Python with Textual, or stretching loom's fzf `loom ui`.** These need no build,
  but the user chose Rust. fzf cannot give two-line rows, separate click zones,
  inline menus or a narrow redraw mode.

## Consequences

A change to the program reaches the boxes only with a fork release, or through
`dogfood.sh build` on the primary mac. That is why glyphs, colours and per-line
tokens are config rather than code. Windows has no `herdr-sidebar` asset and keeps
the native sidebar.
