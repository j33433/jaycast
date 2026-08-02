#!/bin/bash
# Regenerate all raster assets from the plain SVG sources in art/.
#
# Requires: rsvg-convert (librsvg) and cwebp (libwebp).
#
# Outputs per trail:
#   art/{trail}-plain.webp   1024x1024, full artboard viewBox
#   art/{trail}-small.webp   128x128,   trimmed square crop
#   assets/favicon-{trail}.png 180x180, trimmed square crop
#
# The trimmed sizes share one square crop (viewBox "260 272 1573 1573")
# so the three trail marks stay aligned and fill the full height.

set -euo pipefail

command -v rsvg-convert >/dev/null 2>&1 || { echo "error: rsvg-convert not found (librsvg)"; exit 1; }
command -v cwebp >/dev/null 2>&1 || { echo "error: cwebp not found (libwebp)"; exit 1; }

cd "$(dirname "$0")/.."

CROP_VIEWBOX="260 272 1573 1573"

tmp=$(mktemp -d /tmp/clankbox.XXXXXX)
trap 'rm -rf "$tmp"' EXIT

for trail in jaycast gatorcast eaglecast; do
  svg="art/${trail}-plain.svg"

  # 1024px plain, full artboard.
  rsvg-convert -w 1024 -h 1024 -o "$tmp/${trail}-plain.png" "$svg"
  cwebp -quiet -q 95 "$tmp/${trail}-plain.png" -o "art/${trail}-plain.webp"

  # Trimmed crop for the small icon and favicon.
  sed "s/viewBox=\"[^\"]*\"/viewBox=\"$CROP_VIEWBOX\"/" "$svg" > "$tmp/${trail}-crop.svg"
  rsvg-convert -w 128 -h 128 -o "$tmp/${trail}-128.png" "$tmp/${trail}-crop.svg"
  cwebp -quiet -q 95 "$tmp/${trail}-128.png" -o "art/${trail}-small.webp"
  rsvg-convert -w 180 -h 180 -o "assets/favicon-${trail}.png" "$tmp/${trail}-crop.svg"
done

echo "regenerated all icons from art/*-plain.svg"
