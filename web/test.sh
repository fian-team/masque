#!/usr/bin/env bash
# The stage's gate: native host tests, then the wasm build under node
# compared pixel-for-pixel with the native render. Build first (build.sh).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
golden="$root/target/stage-golden.rgba"
(cd "$root" && MASQUE_STAGE_GOLDEN="$golden" cargo test -q -p masque-web)
# shellcheck disable=SC1091
source "$here/env.sh"
node "$here/tests/smoke.mjs" "$golden"
