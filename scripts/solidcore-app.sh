#!/usr/bin/env bash
# Experimental side-by-side macOS build of SolidCraft with solidcore wired in (PR #8526's
# brett19:solidcore line), installed as "SolidCraft Solidcore.app" next to a released SolidCraft.
#
#   scripts/solidcore-app.sh update    merge the latest brett19/solidcore into this branch
#   scripts/solidcore-app.sh build     fetch SMLib, build the app and the CLI (release)
#   scripts/solidcore-app.sh install   assemble the bundle and install it into $APP_DIR
#   scripts/solidcore-app.sh all       update, build, install
#   scripts/solidcore-app.sh status    what is built from what
#
# Settings come from the environment or from `.solidcore-app.env` at the repository root
# (local, not committed):
#   SOLIDCORE_DIR    a solidcore checkout to build against (its crates/solidcore), passed as a
#                    cargo `--config` patch so no tracked file changes. Unset: the pinned revision
#                    in Cargo.toml. A checkout that does not build falls back to the pin.
#   KERNEL_ROUTES    families moved to solidcore by default (SOLIDCRAFT_KERNEL_ROUTES), default
#                    `blend=solidcore`. The app's own preference and the environment still apply.
#   APP_DIR          where the bundle goes, default /Applications.
#   JOBS             cargo jobs, default 8.
#
# Isolation from the released app: its own bundle name and id, no document-type claims (double
# clicking a design keeps opening the release), and its own settings, recovery, crash, control and
# data folders under "~/Library/Application Support/SolidCraft Solidcore" (LSEnvironment).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [ -f "$ROOT/.solidcore-app.env" ]; then
  # shellcheck disable=SC1091
  . "$ROOT/.solidcore-app.env"
fi
SOLIDCORE_DIR="${SOLIDCORE_DIR:-}"
KERNEL_ROUTES="${KERNEL_ROUTES:-blend=solidcore}"
APP_DIR="${APP_DIR:-/Applications}"
JOBS="${JOBS:-8}"
APP_NAME="SolidCraft Solidcore"
BUNDLE_ID="io.tangencylabs.solidcraft.solidcore"
DATA_ROOT="$HOME/Library/Application Support/$APP_NAME"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
STAMP="$TARGET_DIR/solidcore-app.source"
UPSTREAM_REMOTE="${UPSTREAM_REMOTE:-brett19}"
UPSTREAM_BRANCH="${UPSTREAM_BRANCH:-solidcore}"
SOLIDCORE_GIT="https://github.com/tangency-labs/solidcore"

[ "$(uname -s)" = Darwin ] || { echo "macOS only" >&2; exit 2; }

update() {
  if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "update: the working tree has changes; commit or stash them first" >&2
    exit 1
  fi
  git fetch "$UPSTREAM_REMOTE" "$UPSTREAM_BRANCH"
  # Merge, never rebase: the branch is pushed.
  git merge --no-edit "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH"
}

pinned_rev() {
  sed -n 's/^solidcore = { git = "[^"]*", rev = "\([0-9a-f]*\)".*/\1/p' Cargo.toml
}

build() {
  bash scripts/fetch-smlib.sh
  local lock_backup source
  lock_backup="$(mktemp)"
  cp Cargo.lock "$lock_backup"
  # A path patch rewrites Cargo.lock's solidcore entry: put the tracked one back afterwards.
  trap 'cp "$lock_backup" "$ROOT/Cargo.lock"; rm -f "$lock_backup"' RETURN
  local cargo=(nice -n 10 cargo build --release -j "$JOBS" -p solidcraft -p solidcraft-cli)
  source="pinned $(pinned_rev)"
  if [ -n "$SOLIDCORE_DIR" ]; then
    if [ ! -f "$SOLIDCORE_DIR/crates/solidcore/Cargo.toml" ]; then
      echo "build: SOLIDCORE_DIR=$SOLIDCORE_DIR has no crates/solidcore" >&2
      exit 1
    fi
    local sha dirty=""
    sha="$(git -C "$SOLIDCORE_DIR" rev-parse --short=12 HEAD)"
    if [ -n "$(git -C "$SOLIDCORE_DIR" status --porcelain --untracked-files=no -- crates/solidcore)" ]; then dirty="+dirty"; fi
    echo "==> building against local solidcore $SOLIDCORE_DIR @ $sha$dirty"
    if "${cargo[@]}" --config "patch.\"$SOLIDCORE_GIT\".solidcore.path=\"$SOLIDCORE_DIR/crates/solidcore\""; then
      source="local $sha$dirty ($SOLIDCORE_DIR)"
    else
      echo "==> local solidcore does not build with this branch; falling back to the pinned revision" >&2
      cp "$lock_backup" Cargo.lock
      "${cargo[@]}" --locked
    fi
  else
    "${cargo[@]}" --locked
  fi
  mkdir -p "$TARGET_DIR"
  printf '%s\n' "solidcraft $(git rev-parse --short=12 HEAD); solidcore $source" >"$STAMP"
  echo "==> built: $(cat "$STAMP")"
}

plist() {
  /usr/libexec/PlistBuddy -c "$1" "$2"
}

install_app() {
  [ -f "$STAMP" ] || { echo "install: nothing built yet (run build)" >&2; exit 1; }
  local work="$TARGET_DIR/solidcore-app" app info version
  app="$work/$APP_NAME.app"
  info="$app/Contents/Info.plist"
  version="$(awk '/^\[/ { p = ($0 == "[workspace.package]"); next } p && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }' Cargo.toml)"
  rm -rf "$work"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/Licenses"
  cp "$TARGET_DIR/release/solidcraft" "$app/Contents/MacOS/SolidCraft"
  cp "$TARGET_DIR/release/solidcraft-cli" "$app/Contents/MacOS/solidcraft-cli"
  cp assets/icons/app/macos/exports/Assets.car "$app/Contents/Resources/Assets.car"
  cp assets/icons/app/macos/exports/icon.icns "$app/Contents/Resources/SolidCraft.icns"
  for f in README.md LICENSE-MIT LICENSE-APACHE NOTICE THIRD-PARTY-LICENSES.txt; do
    if [ -f "$f" ]; then cp "$f" "$app/Contents/Resources/Licenses/"; fi
  done
  sed -e "s/@VERSION@/$version/g" -e "s/@SHORT_VERSION@/${version%%-*}/g" assets/platforms/macos/Info.plist.in >"$info"
  plist "Set :CFBundleName $APP_NAME" "$info"
  plist "Set :CFBundleDisplayName $APP_NAME" "$info"
  plist "Set :CFBundleIdentifier $BUNDLE_ID" "$info"
  # The release owns .solidcraft files; this build reads older design formats only.
  plist "Delete :CFBundleDocumentTypes" "$info"
  plist "Add :SolidcoreSource string $(cat "$STAMP")" "$info"
  plist "Add :LSEnvironment dict" "$info"
  plist "Add :LSEnvironment:SOLIDCRAFT_SETTINGS_DIR string $DATA_ROOT/settings" "$info"
  plist "Add :LSEnvironment:SOLIDCRAFT_RECOVERY_DIR string $DATA_ROOT/recovery" "$info"
  plist "Add :LSEnvironment:SOLIDCRAFT_CRASH_DIR string $DATA_ROOT/crashes" "$info"
  plist "Add :LSEnvironment:SOLIDCRAFT_CONTROL_DIR string $DATA_ROOT/control" "$info"
  plist "Add :LSEnvironment:SOLIDCRAFT_DATA_DIR string $DATA_ROOT" "$info"
  plist "Add :LSEnvironment:SOLIDCRAFT_KERNEL_ROUTES string $KERNEL_ROUTES" "$info"
  plutil -lint "$info"
  printf 'APPL????' >"$app/Contents/PkgInfo"
  # Ad-hoc signature (Apple silicon refuses unsigned code): nested code first.
  codesign --force --sign - --timestamp=none --identifier "$BUNDLE_ID-cli" "$app/Contents/MacOS/solidcraft-cli"
  codesign --force --sign - --timestamp=none "$app"
  codesign --verify --strict --deep "$app"
  local dest="$APP_DIR/$APP_NAME.app"
  case "$dest" in */SolidCraft.app) echo "refusing to overwrite $dest" >&2; exit 1 ;; esac
  mkdir -p "$DATA_ROOT"
  rm -rf "$dest"
  ditto "$app" "$dest"
  # LaunchServices caches Info.plist (LSEnvironment among it): register the new copy.
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$dest"
  echo "==> installed $dest ($(cat "$STAMP"))"
}

status() {
  echo "branch:    $(git rev-parse --abbrev-ref HEAD) @ $(git rev-parse --short=12 HEAD)"
  echo "upstream:  $UPSTREAM_REMOTE/$UPSTREAM_BRANCH @ $(git rev-parse --short=12 "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH" 2>/dev/null || echo '?')"
  echo "pin:       solidcore $(pinned_rev)"
  echo "local:     ${SOLIDCORE_DIR:-<none, pinned>}"
  echo "routes:    $KERNEL_ROUTES"
  echo "built:     $(cat "$STAMP" 2>/dev/null || echo '<nothing>')"
  local dest="$APP_DIR/$APP_NAME.app"
  if [ -d "$dest" ]; then
    echo "installed: $dest ($(/usr/libexec/PlistBuddy -c 'Print :SolidcoreSource' "$dest/Contents/Info.plist" 2>/dev/null))"
  else
    echo "installed: <no>"
  fi
}

case "${1:-}" in
  update) update ;;
  build) build ;;
  install) install_app ;;
  all) update && build && install_app ;;
  status) status ;;
  *) sed -n '2,25p' "$0" >&2; exit 2 ;;
esac
