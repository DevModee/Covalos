#!/usr/bin/env bash
# Writes THIRD_PARTY_NOTICES.txt and a licenses/ folder for the production dependencies
# of one target, the macOS/Linux counterpart of the notices step in
# build-windows-installer.ps1. Dev-only dependency edges are skipped.
#
# Usage: tools/third-party-notices.sh <target-triple> <output-dir>
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <target-triple> <output-dir>" >&2
  exit 2
fi
target="$1"
out="$2"
repo="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$out/licenses"

packages="$(cargo metadata --locked --format-version 1 --filter-platform "$target" --manifest-path "$repo/Cargo.toml" | jq -r '
  (.resolve.nodes
    | map({key: .id, value: [.deps[] | select(any(.dep_kinds[]; .kind != "dev")) | .pkg]})
    | from_entries) as $graph
  | .resolve.root as $root
  | ({seen: {}, queue: [$root]}
      | until(.queue | length == 0;
          .queue[0] as $id
          | .queue |= .[1:]
          | if .seen[$id] then . else .seen[$id] = true | .queue += ($graph[$id] // []) end)
      | .seen) as $reachable
  | .packages[]
  | select($reachable[.id] and .id != $root)
  | [.name, .version, (.license // "see license files"), (.repository // ""), (.manifest_path | sub("/Cargo.toml$"; ""))]
  | @tsv' | sort -u)"

notices="$out/THIRD_PARTY_NOTICES.txt"
{
  echo "COVALOS - Third-party notices"
  echo "Fonts: Geist and IBM Plex Mono (SIL OFL). UI: Lucide (ISC). File icons: Material Icon Theme and vscode-icons (MIT)."
  echo "Provider marks identify their respective integrations and remain the property of their owners."
  echo "The license texts in licenses/ apply to the named third-party components. They do not grant a license to COVALOS itself."
} > "$notices"

while IFS=$'\t' read -r name version license repository dir; do
  [ -n "$name" ] || continue
  printf '\n%s %s | %s | %s\n' "$name" "$version" "$license" "$repository" >> "$notices"
  for file in "$dir"/LICENSE* "$dir"/LICENCE* "$dir"/COPYING* "$dir"/NOTICE*; do
    if [ -f "$file" ]; then
      cp "$file" "$out/licenses/$name-$version-$(basename "$file")"
    fi
  done
done <<< "$packages"

for asset in assets/fonts/Geist-OFL.txt assets/fonts/IBMPlexMono-OFL.txt \
  assets/icons/ui/LICENSE assets/icons/material/LICENSE assets/icons/vscode/LICENSE \
  assets/brands/PROVENANCE.md; do
  if [ -f "$repo/$asset" ]; then
    cp "$repo/$asset" "$out/licenses/$(echo "$asset" | tr '/' '-')"
  fi
done

echo "Wrote $notices"
