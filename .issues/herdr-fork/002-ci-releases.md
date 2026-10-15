---
id: herdr-fork-ci-releases
title: Fork CI builds and publishes releases
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee: saitonakamura
claimed_by: macos/w6:p3/78b5ca1a-5afc-4903-a949-deb1bda5aefa
---

## Question

Adapt upstream's `.github/workflows/release.yml` so a tag on the fork builds and publishes a GitHub release with `herdr-linux-x86_64`, `herdr-linux-aarch64`, `herdr-macos-x86_64`, `herdr-macos-aarch64` and a Windows asset, plus checksums.

- Versions are `<upstream>+fork.N` (build metadata, so semver comparisons such as a plugin's `min_herdr_version` are unaffected). Check every place that parses or prints the version.
- Fork builds never update themselves: disable `herdr update` and the background version check (fail with a message pointing at brew / the release page). Keep `manifest_check`.
- Disable or neuter upstream workflows that would act on herdr.dev, preview channels, website deploys or issue closing.
- Keep `LICENSE` and notices in every asset.
- First release: upstream 0.9.5 plus only the above, as `0.9.5+fork.1`.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch
