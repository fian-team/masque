#!/usr/bin/env bash
# Build the browser stage: web/dist/ogham_stage.{js,wasm} + manifest.json.
# See docs/internal/WEB_STAGE.md, Phase 4.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
# shellcheck disable=SC1091
source "$here/env.sh"

# Every `stage_*` export in main.rs, plus the heap allocator the page
# writes strings through.
exports=$(grep -oE 'extern "C" fn stage_[a-z_]+' "$here/src/main.rs" \
    | sed -E 's/.* fn /_/' | paste -sd, -)
exports="[$exports,_malloc,_free]"

link=(
    -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createStage
    -sENVIRONMENT=web,node
    -sALLOW_MEMORY_GROWTH=1
    "-sEXPORTED_FUNCTIONS=$exports"
    "-sEXPORTED_RUNTIME_METHODS=[cwrap,HEAPU8]"
)
rustflags=""
for arg in "${link[@]}"; do rustflags+=" -C link-arg=$arg"; done

cd "$root"
RUSTFLAGS="$rustflags" cargo build -p ogham-web --bin ogham_stage \
    --profile stage --target wasm32-unknown-emscripten

out="$root/target/wasm32-unknown-emscripten/stage"
dist="$here/dist"
mkdir -p "$dist"
cp "$out/ogham_stage.js" "$dist/"
"$EMSDK/upstream/bin/wasm-opt" -Oz --all-features "$out/ogham_stage.wasm" -o "$dist/ogham_stage.wasm"

wasm_bytes=$(stat -c %s "$dist/ogham_stage.wasm")
brotli_bytes=$(brotli -q 11 -c "$dist/ogham_stage.wasm" | wc -c)
cat > "$dist/manifest.json" <<JSON
{
  "commit": "$(git -C "$root" rev-parse --short HEAD)$(git -C "$root" diff --quiet HEAD || echo -dirty)",
  "built": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "wasm_bytes": $wasm_bytes,
  "wasm_brotli_bytes": $brotli_bytes
}
JSON
cat "$dist/manifest.json"
