# The sidebar is a pane running the user's program

The fork's user wants their own sidebar: a spawn tree, clickable rows, their own
row layout, every pane listed. Rather than rewrite herdr's native sidebar in Rust,
the fork lets one terminal pane, owned by the server, take the expanded sidebar's
place in every workspace and tab. It runs a program from config (loom's), receives
mouse events, takes keyboard focus on a key, and redraws on herdr events. The
native sidebar stays as the fallback and the collapsed and mobile layouts stay
native.

## Considered Options

- **Rewrite the native sidebar.** Best fidelity, but upstream's sidebar code
  changes heavily and often, so every merge would conflict, and every tweak would
  be Rust.
- **Plugin-declared rows rendered natively.** A middle path, but it needs a new
  protocol surface for row trees, styles and clicks that nothing else uses.

## Consequences

The sidebar is only as good as a terminal program can draw, and it has one width
for every attached client.
