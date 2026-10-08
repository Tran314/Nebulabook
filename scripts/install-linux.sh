#!/usr/bin/env bash
# Optional per-user installer. Application data is never read, moved or removed.
set -euo pipefail

fail() { printf 'install-linux: %s\n' "$*" >&2; exit 1; }
usage() {
  printf '%s\n' 'Usage: ./install-linux.sh [--prefix "$HOME/.local"]' \
    'Installs lib/nebulabook, bin/nebulabook and an XDG application menu entry.' \
    'All destinations must resolve inside your home directory. Never use sudo.'
}

prefix=${HOME:?HOME is required}/.local
while (($#)); do
  case $1 in
    --prefix) (($# >= 2)) || fail '--prefix needs a directory'; prefix=$2; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; fail "Unknown argument: $1" ;;
  esac
done
[[ $(uname -s) == Linux ]] || fail 'This installer supports Linux only'
[[ $EUID != 0 ]] || fail 'Run as your normal user, without sudo'
[[ -x /usr/bin/env ]] || fail 'Required command not found: /usr/bin/env'
for command in realpath sha256sum install mktemp mv ln od; do
  command -v "$command" >/dev/null || fail "Required command not found: $command"
done
home=$(realpath -e -- "$HOME")
[[ $home != / ]] || fail 'HOME must not be the filesystem root'
source_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
for file in nebulabook README.md LICENSE nebulabook.desktop SHA256SUMS; do
  [[ -f $source_dir/$file ]] || fail "Missing package file: $file; run the installer from the extracted package"
done
(cd -- "$source_dir" && sha256sum --check --strict --quiet SHA256SUMS) || fail 'Package checksum verification failed'
case $(uname -m) in
  x86_64) expected_machine=62 ;;
  aarch64) expected_machine=183 ;;
  *) fail 'Supported native architectures are x86_64 and aarch64' ;;
esac
machine=$(od -An -t u2 -j 18 -N 2 -- "$source_dir/nebulabook")
[[ ${machine//[[:space:]]/} == "$expected_machine" ]] || fail 'Package architecture does not match this computer'

# Check resolved paths, including existing symlinks, before creating anything.
inside_home() {
  local path=$1
  [[ $path == /* ]] || fail "Destination must be absolute: $path"
  [[ ! $path =~ [[:cntrl:]] ]] || fail 'Control characters in paths are not supported'
  path=$(realpath -m -- "$path")
  [[ $path == "$home"/* ]] || fail "Destination must stay inside HOME: $path"
  printf '%s' "$path"
}
prefix=$(inside_home "$prefix")
install_dir=$(inside_home "$prefix/lib/nebulabook")
bin_dir=$(inside_home "$prefix/bin")
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
applications_dir=$(inside_home "$data_home/applications")
mkdir -p -- "$install_dir" "$bin_dir" "$applications_dir"

# Desktop entries have a string-escape layer followed by Exec argument escaping.
# Quote the whole path, escape shell-reserved characters, then double backslashes
# for the outer string layer; %% is the desktop literal-percent escape.
# GNU env -C keeps special directory characters out of executable-name lookup
# and NAME=VALUE parsing. It does not evaluate a shell command (Ubuntu 22.04+).
desktop_exec() {
  local value=$1
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  value=${value//\$/\\\$}
  value=${value//\`/\\\`}
  value=${value//%/%%}
  value=${value//\\/\\\\}
  printf '"%s"' "$value"
}

pending=()
cleanup() { for temporary in "${pending[@]}"; do rm -f -- "$temporary"; done; }
trap cleanup EXIT
# Same-directory temporary files make upgrades safe even when the old app runs.
for file in nebulabook README.md LICENSE; do
  temporary=$(mktemp "$install_dir/.install.XXXXXX")
  pending+=("$temporary")
  mode=644; [[ $file != nebulabook ]] || mode=755
  install -m "$mode" -- "$source_dir/$file" "$temporary"
  mv -fT -- "$temporary" "$install_dir/$file"
done
executable=$install_dir/nebulabook
temporary=$(mktemp "$bin_dir/.nebulabook-link.XXXXXX")
pending+=("$temporary")
rm -- "$temporary"
ln -s -- "$executable" "$temporary"
mv -fT -- "$temporary" "$bin_dir/nebulabook"

temporary=$(mktemp --suffix=.desktop "$applications_dir/.nebulabook-desktop.XXXXXX")
pending+=("$temporary")
while IFS= read -r line || [[ -n $line ]]; do
  case $line in
    Exec=*) printf 'Exec=/usr/bin/env -C %s -- ./nebulabook\n' "$(desktop_exec "$install_dir")" ;;
    *) printf '%s\n' "$line" ;;
  esac
done < "$source_dir/nebulabook.desktop" > "$temporary"
chmod 644 -- "$temporary"
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$temporary"; fi
mv -fT -- "$temporary" "$applications_dir/nebulabook.desktop"
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$applications_dir" || printf '%s\n' 'Menu cache refresh failed; log out and back in if needed.' >&2
fi
printf 'Installed: %s\nMenu entry: %s\n' "$executable" "$applications_dir/nebulabook.desktop"
printf '%s\n' 'Open Nebulabook from your application menu. Your note data was not changed.'
printf 'Terminal launch: %s\n' "$bin_dir/nebulabook"
