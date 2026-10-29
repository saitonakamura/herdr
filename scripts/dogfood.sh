#!/usr/bin/env bash
# Install a fork build (herdr, herdr-sidebar and loom-rust) for the dogfood side session, at
# its own prefix. Never touches the Homebrew herdr or any running server: each
# binary is copied in and renamed over the old one, so a running side server
# keeps its old inode.
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

stage() { # path name: copy in beside the old one and check it runs
  cp "$1" "$prefix/bin/$2.new"
  chmod 755 "$prefix/bin/$2.new"
  "$prefix/bin/$2.new" --version >/dev/null || {
    rm -f "$prefix/bin/herdr.new" "$prefix/bin/herdr-sidebar.new" "$prefix/bin/loom-rust.new"
    die "the new $2 does not run"
  }
}

install_binary() { # herdr-path sidebar-path-or-empty source-description loom-path-or-empty
  mkdir -p "$prefix/bin"
  # Check every staged binary before replacing any installed one.
  stage "$1" herdr
  # the sidebar program (ADR 0006); releases before it have none
  [ -z "$2" ] || stage "$2" herdr-sidebar
  [ -z "$4" ] || stage "$4" loom-rust
  # renamed over the old ones, so a running server and sidebar pane keep their inodes
  mv -f "$prefix/bin/herdr.new" "$prefix/bin/herdr"
  [ -z "$2" ] || mv -f "$prefix/bin/herdr-sidebar.new" "$prefix/bin/herdr-sidebar"
  [ -z "$4" ] || mv -f "$prefix/bin/loom-rust.new" "$prefix/bin/loom-rust"
  printf '%s\ninstalled %s\n' "$3" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$prefix/SOURCE"
  status
}

cmd_release() {
  local tag="${1:-}" asset sidebar loom patterns names
  [ -n "$tag" ] || tag=$(gh release list -R "$repo_slug" --limit 1 --json tagName -q '.[0].tagName')
  asset=$(asset_name)
  sidebar=${asset/herdr-/herdr-sidebar-}
  loom=${asset/herdr-/loom-rust-}
  patterns=(-p "$asset" -p SHA256SUMS)
  # releases before herdr-fork 030 ship no sidebar; rolling back to one keeps the installed sidebar
  names=$(gh release view "$tag" -R "$repo_slug" --json assets -q '.assets[].name') || die "no release $tag"
  if grep -qx "$sidebar" <<<"$names"; then
    patterns+=(-p "$sidebar")
  else
    echo "dogfood: release $tag has no $sidebar; leaving herdr-sidebar as it is" >&2
    sidebar=
  fi
  if grep -qx "$loom" <<<"$names"; then
    patterns+=(-p "$loom")
  else
    echo "dogfood: release $tag has no $loom; leaving loom-rust as it is" >&2
    loom=
  fi
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  gh release download "$tag" -R "$repo_slug" -D "$tmp" "${patterns[@]}"
  (cd "$tmp" && sha256sum --check --ignore-missing --strict SHA256SUMS) || die "checksum mismatch for $asset${sidebar:+ or $sidebar}"
  install_binary "$tmp/$asset" "${sidebar:+$tmp/$sidebar}" "release $tag ($asset)" "${loom:+$tmp/$loom}"
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
  install_binary "$build_wt/target/release/herdr" "$build_wt/target/release/herdr-sidebar" "$desc" "$build_wt/target/release/loom-rust"
}

status() {
  [ -x "$prefix/bin/herdr" ] || { echo "nothing installed at $prefix"; return; }
  echo "$prefix/bin/herdr: $("$prefix/bin/herdr" --version)"
  if [ -x "$prefix/bin/herdr-sidebar" ]; then
    echo "$prefix/bin/herdr-sidebar: $("$prefix/bin/herdr-sidebar" --version)"
  fi
  if [ -x "$prefix/bin/loom-rust" ]; then
    echo "$prefix/bin/loom-rust: $("$prefix/bin/loom-rust" --version)"
  fi
  cat "$prefix/SOURCE" 2>/dev/null || true
}

case "${1:-}" in
  release) shift; cmd_release "$@" ;;
  build) shift; cmd_build "$@" ;;
  status) status ;;
  *) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
