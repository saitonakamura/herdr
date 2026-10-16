#!/usr/bin/env bash
# Install a fork build for the dogfood side session, at its own prefix. Never
# touches the Homebrew herdr or any running server: the binary is copied in and
# renamed over the old one, so a running side server keeps its old inode.
#
#   scripts/dogfood.sh release [<tag>]        a fork release asset, checked against SHA256SUMS
#   scripts/dogfood.sh build [<pr|branch>...] a release build of origin/master plus unmerged PRs
#   scripts/dogfood.sh status                 what is installed, and from where
#
# HERDR_DOGFOOD_PREFIX overrides the prefix (default ~/.local/opt/herdr-fork).
set -euo pipefail

prefix="${HERDR_DOGFOOD_PREFIX:-$HOME/.local/opt/herdr-fork}"
repo_slug=saitonakamura/herdr
repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
# a dedicated worktree, kept between builds so its target/ stays warm
build_wt="${HERDR_DOGFOOD_BUILD_DIR:-$(dirname "$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir)")/../herdr-worktrees/dogfood-build}"

die() { echo "dogfood: $*" >&2; exit 1; }

asset_name() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) echo herdr-macos-aarch64 ;;
    Darwin-x86_64) echo herdr-macos-x86_64 ;;
    Linux-aarch64) echo herdr-linux-aarch64 ;;
    Linux-x86_64) echo herdr-linux-x86_64 ;;
    *) die "no release asset for $(uname -s)-$(uname -m)" ;;
  esac
}

install_binary() { # path source-description
  mkdir -p "$prefix/bin"
  cp "$1" "$prefix/bin/herdr.new"
  chmod 755 "$prefix/bin/herdr.new"
  "$prefix/bin/herdr.new" --version >/dev/null || die "the new binary does not run"
  mv -f "$prefix/bin/herdr.new" "$prefix/bin/herdr"
  printf '%s\ninstalled %s\n' "$2" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$prefix/SOURCE"
  status
}

cmd_release() {
  local tag="${1:-}" asset
  [ -n "$tag" ] || tag=$(gh release list -R "$repo_slug" --limit 1 --json tagName -q '.[0].tagName')
  asset=$(asset_name)
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  gh release download "$tag" -R "$repo_slug" -D "$tmp" -p "$asset" -p SHA256SUMS
  (cd "$tmp" && sha256sum --check --ignore-missing --strict SHA256SUMS) || die "checksum mismatch for $asset"
  install_binary "$tmp/$asset" "release $tag ($asset)"
}

# a PR number resolves to its head branch; anything else is taken as a branch name
resolve_ref() {
  if [[ "$1" =~ ^#?([0-9]+)$ ]]; then
    gh pr view "${BASH_REMATCH[1]}" -R "$repo_slug" --json headRefName -q .headRefName
  else
    echo "$1"
  fi
}

cmd_build() {
  local refs=() ref desc
  git -C "$repo" fetch -q origin
  for ref in "$@"; do refs+=("origin/$(resolve_ref "$ref")"); done
  if [ -d "$build_wt" ]; then
    git -C "$build_wt" reset -q --hard
    git -C "$build_wt" checkout -q --detach origin/master
  else
    git -C "$repo" worktree add -q --detach "$build_wt" origin/master
  fi
  desc="build origin/master@$(git -C "$build_wt" rev-parse --short HEAD)"
  for ref in "${refs[@]}"; do desc+=" + $ref@$(git -C "$repo" rev-parse --short "$ref")"; done
  # --no-commit: nothing is committed, so nothing needs signing, and the next build resets it away
  if [ ${#refs[@]} -gt 0 ]; then
    git -C "$build_wt" merge -q --no-ff --no-commit "${refs[@]}" || die "the branches do not merge cleanly onto master"
  fi
  (cd "$build_wt" && cargo build --release --locked)
  install_binary "$build_wt/target/release/herdr" "$desc"
}

status() {
  [ -x "$prefix/bin/herdr" ] || { echo "nothing installed at $prefix"; return; }
  echo "$prefix/bin/herdr: $("$prefix/bin/herdr" --version)"
  cat "$prefix/SOURCE" 2>/dev/null || true
}

case "${1:-}" in
  release) shift; cmd_release "$@" ;;
  build) shift; cmd_build "$@" ;;
  status) status ;;
  *) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
