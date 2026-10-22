# The collapsed sidebar is the sidebar pane too

ADR 0002 left the collapsed layout native, so collapsing the sidebar swapped the
user's program for herdr's status rail. The user wants a narrow spawn tree
instead. When a sidebar pane is configured, collapsing now shrinks that pane to
`pane_collapsed_width`, and the program redraws a narrow tree (one cell per depth,
the state glyph, the start of the name) and keeps its click targets. The
`hidden` collapsed mode still gives the sidebar zero width, and mobile stays
native. This supersedes 0002 only where it says the collapsed layout stays native.

## Considered Options

- **Keep the native compact rail.** No new width to manage. Rejected because the
  rail shows one dot per agent with no tree, so collapsing would lose the
  structure the sidebar pane exists to show.

## Consequences

The program must draw well at every width from about 6 columns up, and the native
rail is now reached only when no sidebar pane is configured or while its process
restarts.
