#!/usr/bin/env bash
# Fetch or build llama-server for macOS (arm64/x86_64) into local_llm/bin/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/local_llm/bin"
mkdir -p "$OUT_DIR"

ARCH="$(uname -m)"
case "$ARCH" in
  arm64) ASSET_ARCH="macos-arm64" ;;
  x86_64) ASSET_ARCH="macos-x64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

echo "[FlowSight] Looking up latest llama.cpp release asset for $ASSET_ARCH…"
API="https://api.github.com/repos/ggml-org/llama.cpp/releases/latest"
JSON="$(/usr/bin/curl -fsSL "$API")"
ASSET_URL="$(
  printf '%s' "$JSON" \
    | /usr/bin/grep -oE "\"browser_download_url\": \"[^\"]*${ASSET_ARCH}[^\"]*\"" \
    | /usr/bin/head -n1 \
    | /usr/bin/sed -E 's/.*"browser_download_url": "([^"]+)".*/\1/'
)" || true

extract_archive() {
  local archive="$1"
  local dest="$2"
  mkdir -p "$dest"
  case "$archive" in
    *.tar.gz|*.tgz)
      /usr/bin/tar -xzf "$archive" -C "$dest"
      ;;
    *.zip)
      /usr/bin/unzip -q "$archive" -d "$dest"
      ;;
    *)
      echo "[FlowSight] Unknown archive format: $archive"
      return 1
      ;;
  esac
}

if [[ -n "${ASSET_URL:-}" ]]; then
  echo "[FlowSight] Downloading $ASSET_URL"
  EXT="bin"
  case "$ASSET_URL" in
    *.tar.gz) EXT="tar.gz" ;;
    *.tgz) EXT="tgz" ;;
    *.zip) EXT="zip" ;;
  esac
  ARCHIVE="$TMP/llama.$EXT"
  /usr/bin/curl -fL "$ASSET_URL" -o "$ARCHIVE"
  extract_archive "$ARCHIVE" "$TMP/extract"
  SERVER="$(/usr/bin/find "$TMP/extract" -type f -name 'llama-server' | /usr/bin/head -n1 || true)"
  if [[ -n "$SERVER" ]]; then
    /bin/cp -f "$SERVER" "$OUT_DIR/llama-server"
    /bin/chmod +x "$OUT_DIR/llama-server"
    SERVER_DIR="$(/usr/bin/dirname "$SERVER")"
    /bin/cp -f "$SERVER_DIR"/*.dylib "$OUT_DIR/" 2>/dev/null || true
    # Some builds put dylibs in a lib/ sibling.
    /bin/cp -f "$SERVER_DIR/../lib"/*.dylib "$OUT_DIR/" 2>/dev/null || true
    echo "[FlowSight] Installed $OUT_DIR/llama-server"
    "$OUT_DIR/llama-server" --version 2>/dev/null || true
    exit 0
  fi
  echo "[FlowSight] Archive had no llama-server; will build from source."
fi

echo "[FlowSight] Building llama.cpp from source (Metal)…"
if ! command -v cmake >/dev/null 2>&1; then
  echo "cmake is required to build llama.cpp. Install with: brew install cmake"
  exit 1
fi

/usr/bin/git clone --depth 1 https://github.com/ggml-org/llama.cpp.git "$TMP/llama.cpp"
cmake -S "$TMP/llama.cpp" -B "$TMP/build" -DGGML_METAL=ON -DLLAMA_BUILD_SERVER=ON -DCMAKE_BUILD_TYPE=Release
cmake --build "$TMP/build" --config Release -j "$(sysctl -n hw.ncpu)" --target llama-server
/bin/cp -f "$TMP/build/bin/llama-server" "$OUT_DIR/llama-server"
/bin/chmod +x "$OUT_DIR/llama-server"
/bin/cp -f "$TMP/build/bin"/*.dylib "$OUT_DIR/" 2>/dev/null || true
echo "[FlowSight] Built $OUT_DIR/llama-server"
