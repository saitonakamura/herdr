---
id: herdr-fork-false-user-input
title: Last user input fires without the user typing
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Follow-up to [029](./029-last-user-input.md). The user asked for this ticket through the dotfiles pane on 2026-10-26.

**Evidence.** Personal pane `w1:pR` (claude, loom node `19ec4db5` `loom-017-grill`) reported at 2026-10-26T18:25:14Z. `pane current` then gave `last_user_input_at_ms` 1793039128819, which is 18:25:28.819Z, fourteen seconds later. The user typed nothing: the `dispatch 071` in the prompt box was Claude's suggestion ghost text. So `loom close` refused the pane as user-held.

**What the server counts today** (029's resolution, `src/server/pane_input.rs`, `src/server/headless.rs`):
- a client-shell `Key` with kind Press or Repeat, a non-empty `TextCommit`, or a non-empty `Paste`;
- terminal-attach bytes that `parse_framed_input` reads as a key, text or paste;
- a clipboard image paste.

Mouse events are not counted there. So a false stamp means something arrived at the server *as* a key, text or paste event. Suspects, none verified:

- **Modifier-only keys.** Under the kitty keyboard protocol with all-keys reporting, a bare Ctrl, Alt, Super or Shift press is a key Press event. Holding Ctrl for Ctrl+hover or Ctrl+click on a link, or Cmd+Tab away, would then stamp the pane. This is the most likely one: it needs no misparse at all.
- **Host-terminal replies decoded as keys** on the client: OSC 10/11 colour replies, DA, kitty keyboard-flag or size replies. Claude Code queries several of these around turn end. Herdr answers the pane's queries itself, so check where a *host* reply to herdr's own queries lands. One the client parser doesn't recognise could come through as Alt+`]` and text.
- **Focus in/out** (`CSI I` / `CSI O`) parsed as keys on either path.
- **Key Repeat or Release confusion**, for example a release reported as Press when `tracks_release` is false.

**Fix direction.** Count only input that the user typed into the pane:
- printable text;
- a non-modifier key press or repeat, Enter and Backspace included;
- a non-empty paste.

Exclude modifier-only keys, terminal replies, focus and mouse. Reproduce first: on a throwaway session, record which `ClientPaneInputEvent`s set the stamp while doing each suspect action (tap Ctrl, Ctrl+hover, Cmd+Tab, focus out and in, an agent turn end with colour queries). Then pin the excluded kinds with an engine-level test. The field's contract (029) stays the same; only what counts as typing narrows.

## Parent

[000-map](./000-map.md)

## Resolution

Merged 2026-10-29: [PR #41](https://github.com/saitonakamura/herdr/pull/41) as `52ccf633`. Built 2026-10-29 by host pi (`input-046`, the first
host-pi dogfood run), finished in the cockpit.

- **Fixed: bare modifiers on the terminal-attach path.** A raw attach client
  (`herdr agent attach`, `herdr terminal attach`) counted a kitty-protocol
  modifier-only press (`CSI 57441..57454 u`, Ctrl/Alt/Shift/Super) as typing.
  `terminal_attach_input_is_typing` now skips `KeyCode::Modifier`. Focus, colour,
  DA, kitty-flag, size and mouse reports and key releases were already excluded;
  tests now pin all of them, at the classifier and through the headless server.
- **The 10-26 incident is not explained by this.** That pane was in the normal
  client shell, which already drops modifier-only keys before forwarding them
  (`src/client/shell/input.rs`, except into a popup pane). Its stamp came from a
  key, text or paste event the client forwarded; which one was not captured, and
  no log records it today. If it shows again, reopen with a trace on the stamp
  first, then reproduce.
