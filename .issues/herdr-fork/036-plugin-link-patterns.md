---
id: herdr-fork-plugin-link-patterns
title: Plugin-declared plain-text link patterns
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Requested by the dotfiles terminal-clickability effort ([its map](https://github.com/saitonakamura/dotfiles/blob/master/.issues/terminal-clickability/000-map.md), 366efd2). That effort builds a `links` plugin in the dotfiles, and the plugin must still run on stock herdr. This ticket only adds a hook herdr lacks; a picker already covers the same ground, so nothing waits on it.

Today Ctrl+click and Ctrl+hover (`src/client/shell/mouse.rs`) treat only OSC 8 links and plain-text `http(s)` URLs as links. A plugin's `[[link_handlers]]` `pattern` (`PluginManifestLinkHandler`, `src/api/schema/plugins.rs`) runs only against a URL herdr has already resolved. So `src/foo.rs:12`, a commit hash or a `.issues` id under the pointer is not a link.

- Let a plugin declare plain-text patterns: regexes matched against the text under the pointer, wrap-aware the way the http(s) detection is now. The plugin wants `path:line[:col]`, stack-trace frames, commit hashes, `.issues` ids, herdr pane ids and loom ids, plus Jira keys and GitLab `!MR` at work.
- A match underlines on Ctrl+hover and dispatches on Ctrl+click to the declaring handler's action, with the matched text in the env (today's `HERDR_PLUGIN_CLICKED_URL`, or a sibling variable).
- Decide the manifest shape: a new field on `[[link_handlers]]`, or a separate table. Existing manifests must keep their meaning.
- Decide precedence: OSC 8, then plugin text patterns, then built-in http(s)? And what happens when two plugins match the same span.
- Matching runs in the client's hover path, which is a multiplicative path (AGENTS.md). Compile patterns once per manifest load, match only the row(s) under the pointer, and allocate nothing when no plugin declares patterns.
- `pane.link.resolve` / `pane.link.activate` are frozen generation-1 methods. A resolution that a stock server cannot produce goes behind a new method or an advertised capability, never a reinterpreted one.

## Parent

[000-map](./000-map.md)

## Resolution
