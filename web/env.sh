# Source this before any `--target wasm32-unknown-emscripten` build.
#
# emsdk 4.0.7 is the version Skia 0.91 pins (skia/bin/activate-emsdk).
# Set EMSDK to its root if it is not at ~/emsdk.
#
# Skia is built from source: rust-skia's prebuilt emscripten archive uses
# emscripten's JS setjmp, Rust's emscripten target links with native wasm
# exceptions, and emcc refuses the mix. The first build takes ~15 minutes
# per target directory. bindgen needs emsdk's compat headers (xlocale.h).
# See docs/internal/WEB_STAGE.md, Phase 1.

: "${EMSDK:=$HOME/emsdk}"
# shellcheck disable=SC1091
source "$EMSDK/emsdk_env.sh" > /dev/null 2>&1
export EMSDK
export FORCE_SKIA_BUILD=1
export SKIA_GN_ARGS='extra_cflags+=["-fwasm-exceptions","-sSUPPORT_LONGJMP=wasm"]'
export BINDGEN_EXTRA_CLANG_ARGS_wasm32_unknown_emscripten="-isystem$EMSDK/upstream/emscripten/system/include/compat"

# Once Skia's third-party sources are synced (the first online build does
# it), build offline from them: `git-sync-deps` otherwise re-fetches gn on
# every build script run, and fails on a flaky connection.
_skia_src=$(ls -d "$HOME"/.cargo/registry/src/*/skia-bindings-0.91.0/skia 2>/dev/null | head -1)
if [ -n "$_skia_src" ] && [ -d "$_skia_src/third_party/externals/freetype/src" ]; then
    export SKIA_SOURCE_DIR="$_skia_src"
fi
unset _skia_src
