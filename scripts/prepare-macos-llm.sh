#!/usr/bin/env bash
# Fetch or build llama-server for macOS into local_llm/bin/.
# Override host detection with:
#   FLOWSIGHT_LLM_ARCH=macos-arm64|macos-x64
# Skip executing the binary (cross-compile CI) with:
#   FLOWSIGHT_SKIP_LLM_RUN=1
# Optional auth for GitHub API rate limits:
#   GITHUB_TOKEN / GH_TOKEN
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/local_llm/bin"
mkdir -p "$OUT_DIR"

if [[ -n "${FLOWSIGHT_LLM_ARCH:-}" ]]; then
  ASSET_ARCH="$FLOWSIGHT_LLM_ARCH"
else
  ARCH="$(uname -m)"
  case "$ARCH" in
    arm64) ASSET_ARCH="macos-arm64" ;;
    x86_64) ASSET_ARCH="macos-x64" ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
  esac
fi

case "$ASSET_ARCH" in
  macos-arm64) WANT_ARCH="arm64" ;;
  macos-x64) WANT_ARCH="x86_64" ;;
  *) echo "FLOWSIGHT_LLM_ARCH must be macos-arm64 or macos-x64 (got: ${ASSET_ARCH})"; exit 1 ;;
esac

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

github_curl() {
  # Prefer token to avoid unauthenticated API 403s on GitHub Actions.
  local url="$1"
  local out="$2"
  local token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
  if [[ -n "$token" ]]; then
    /usr/bin/curl -fsSL \
      -H "Authorization: Bearer ${token}" \
      -H "Accept: application/vnd.github+json" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      "$url" -o "$out"
  else
    /usr/bin/curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      "$url" -o "$out"
  fi
}

verify_llama_arch() {
  local bin="$1"
  local want="$2" # arm64 | x86_64
  local got
  got="$(/usr/bin/lipo -archs "$bin" 2>/dev/null || true)"
  if [[ -z "$got" ]]; then
    got="$(/usr/bin/file -b "$bin")"
  fi
  if ! printf '%s' "$got" | /usr/bin/grep -q "$want"; then
    echo "[FlowSight] ERROR: $bin is '$got' but expected arch $want ($ASSET_ARCH)"
    return 1
  fi
  echo "[FlowSight] Verified $bin arch: $got"
  return 0
}

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

keep_existing_if_valid() {
  if [[ -x "$OUT_DIR/llama-server" ]] && verify_llama_arch "$OUT_DIR/llama-server" "$WANT_ARCH"; then
    echo "[FlowSight] Keeping existing local_llm/bin/llama-server ($ASSET_ARCH)"
    if [[ "${FLOWSIGHT_SKIP_LLM_RUN:-0}" != "1" ]]; then
      "$OUT_DIR/llama-server" --version 2>/dev/null || true
    fi
    return 0
  fi
  return 1
}

echo "[FlowSight] Looking up latest llama.cpp release asset for ${ASSET_ARCH}..."
API="https://api.github.com/repos/ggml-org/llama.cpp/releases/latest"
ASSET_URL=""
if github_curl "$API" "$TMP/latest.json"; then
  ASSET_URL="$(
    /usr/bin/grep -oE "\"browser_download_url\": \"[^\"]*${ASSET_ARCH}[^\"]*\"" "$TMP/latest.json" \
      | /usr/bin/head -n1 \
      | /usr/bin/sed -E 's/.*"browser_download_url": "([^"]+)".*/\1/'
  )" || true
else
  echo "[FlowSight] GitHub API lookup failed (rate limit/403?). Will try existing binary or source build."
fi

install_from_url() {
  local url="$1"
  echo "[FlowSight] Downloading $url"
  EXT="bin"
  case "$url" in
    *.tar.gz) EXT="tar.gz" ;;
    *.tgz) EXT="tgz" ;;
    *.zip) EXT="zip" ;;
  esac
  ARCHIVE="$TMP/llama.$EXT"
  /usr/bin/curl -fL "$url" -o "$ARCHIVE"

  # Only wipe previous bins once we have a download in hand.
  /usr/bin/find "$OUT_DIR" -maxdepth 1 \( -name 'llama-server' -o -name '*.dylib' \) -delete 2>/dev/null || true

  extract_archive "$ARCHIVE" "$TMP/extract"
  SERVER="$(/usr/bin/find "$TMP/extract" -type f -name 'llama-server' | /usr/bin/head -n1 || true)"
  if [[ -z "$SERVER" ]]; then
    echo "[FlowSight] Archive had no llama-server"
    return 1
  fi
  /bin/cp -f "$SERVER" "$OUT_DIR/llama-server"
  /bin/chmod +x "$OUT_DIR/llama-server"
  SERVER_DIR="$(/usr/bin/dirname "$SERVER")"
  /bin/cp -f "$SERVER_DIR"/*.dylib "$OUT_DIR/" 2>/dev/null || true
  /bin/cp -f "$SERVER_DIR/../lib"/*.dylib "$OUT_DIR/" 2>/dev/null || true
  verify_llama_arch "$OUT_DIR/llama-server" "$WANT_ARCH"
  echo "[FlowSight] Installed $OUT_DIR/llama-server ($ASSET_ARCH)"
  if [[ "${FLOWSIGHT_SKIP_LLM_RUN:-0}" != "1" ]]; then
    "$OUT_DIR/llama-server" --version 2>/dev/null || true
  fi
  return 0
}

if [[ -n "${ASSET_URL:-}" ]]; then
  if install_from_url "$ASSET_URL"; then
    exit 0
  fi
  echo "[FlowSight] Download/install failed; falling back."
fi

if keep_existing_if_valid; then
  exit 0
fi

echo "[FlowSight] Building llama.cpp from source (Metal) for ${ASSET_ARCH}..."
if ! command -v cmake >/dev/null 2>&1; then
  echo "cmake is required to build llama.cpp. Install with: brew install cmake"
  exit 1
fi

CMAKE_OSX_ARCH="$WANT_ARCH"
/usr/bin/find "$OUT_DIR" -maxdepth 1 \( -name 'llama-server' -o -name '*.dylib' \) -delete 2>/dev/null || true
/usr/bin/git clone --depth 1 https://github.com/ggml-org/llama.cpp.git "$TMP/llama.cpp"
cmake -S "$TMP/llama.cpp" -B "$TMP/build" \
  -DGGML_METAL=ON \
  -DLLAMA_BUILD_SERVER=ON \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_ARCHITECTURES="$CMAKE_OSX_ARCH"
cmake --build "$TMP/build" --config Release -j "$(sysctl -n hw.ncpu)" --target llama-server
/bin/cp -f "$TMP/build/bin/llama-server" "$OUT_DIR/llama-server"
/bin/chmod +x "$OUT_DIR/llama-server"
/bin/cp -f "$TMP/build/bin"/*.dylib "$OUT_DIR/" 2>/dev/null || true
echo "[FlowSight] Built $OUT_DIR/llama-server ($ASSET_ARCH)"
verify_llama_arch "$OUT_DIR/llama-server" "$WANT_ARCH"
