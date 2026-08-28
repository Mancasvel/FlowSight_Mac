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

# llama.cpp releases ship every CLI tool's helper dylib plus three byte-identical
# aliases of each library (libggml-base.dylib / .0.dylib / .0.20.1.dylib). Tauri
# copies resources with fs::copy, which dereferences symlinks, so the only way to
# keep the bundle small is to ship just the libraries llama-server actually loads.
# Everything is reachable statically (verified with otool: the Metal/BLAS/RPC
# backends are LC_LOAD_DYLIB entries, not dlopen'd), so the link closure is exact.
prune_unreferenced_dylibs() {
  if ! command -v /usr/bin/otool >/dev/null 2>&1; then
    echo "[FlowSight] otool unavailable — keeping every downloaded dylib"
    return 0
  fi
  /usr/bin/python3 - "$OUT_DIR" <<'PY'
import os
import re
import shutil
import subprocess
import sys

out_dir = sys.argv[1]
root_name = "llama-server"
if not os.path.exists(os.path.join(out_dir, root_name)):
    raise SystemExit("[FlowSight] ERROR: llama-server missing before dylib prune")

DEP_COMMANDS = {"LC_LOAD_DYLIB", "LC_LOAD_WEAK_DYLIB", "LC_REEXPORT_DYLIB", "LC_LOAD_UPWARD_DYLIB"}
VERSION_SUFFIX = re.compile(r"(\.\d+)+$")


def load_commands(path):
    """Return (dependency install names, LC_RPATH entries) for a Mach-O file."""
    result = subprocess.run(["/usr/bin/otool", "-l", path], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"[FlowSight] ERROR: otool -l failed for {path}")
    deps, rpaths, command = [], [], None
    for raw in result.stdout.splitlines():
        line = raw.strip()
        if line.startswith("cmd "):
            command = line.split(None, 1)[1]
        elif line.startswith("name ") and command in DEP_COMMANDS:
            deps.append(line[5:].split(" (offset")[0])
        elif line.startswith("path ") and command == "LC_RPATH":
            rpaths.append(line[5:].split(" (offset")[0])
    return deps, rpaths


def library_stem(name):
    return VERSION_SUFFIX.sub("", name[: -len(".dylib")]) if name.endswith(".dylib") else name


entries = {name for name in os.listdir(out_dir) if not name.startswith(".")}
keep = {root_name}
queue = [root_name]
while queue:
    name = queue.pop()
    deps, rpaths = load_commands(os.path.join(out_dir, name))
    for rpath in rpaths:
        if not rpath.startswith(("@loader_path", "@executable_path")):
            raise SystemExit(f"[FlowSight] ERROR: {name} has non-relocatable LC_RPATH '{rpath}'")
    for dep in deps:
        base = os.path.basename(dep)
        if base in entries:
            if base not in keep:
                keep.add(base)
                queue.append(base)
        elif dep.startswith(("@rpath", "@loader_path", "@executable_path")):
            raise SystemExit(f"[FlowSight] ERROR: {name} needs '{dep}' which is missing from {out_dir}")

# A future llama.cpp built with GGML_BACKEND_DL would dlopen its backends instead
# of linking them, making them invisible to otool. Keep any ggml library whose
# family is absent from the link closure rather than silently dropping Metal.
linked_stems = {library_stem(name) for name in keep}
for name in entries - keep:
    if name.startswith("libggml") and library_stem(name) not in linked_stems:
        print(f"[FlowSight] Keeping possibly dlopen'd backend {name}")
        keep.add(name)

for name in sorted(keep):
    path = os.path.join(out_dir, name)
    if os.path.islink(path):
        materialised = path + ".real"
        shutil.copy2(os.path.realpath(path), materialised)
        os.remove(path)
        os.rename(materialised, path)

removed = 0
for name in sorted(entries - keep):
    path = os.path.join(out_dir, name)
    if os.path.islink(path) or os.path.isfile(path):
        os.remove(path)
        removed += 1

print(f"[FlowSight] Bundling {len(keep)} Mach-O files; dropped {removed} unused/duplicate ones")
PY
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
  if [[ ! -x "$OUT_DIR/llama-server" ]]; then
    return 1
  fi
  if verify_llama_arch "$OUT_DIR/llama-server" "$WANT_ARCH"; then
    echo "[FlowSight] Keeping existing local_llm/bin/llama-server ($ASSET_ARCH)"
    if [[ "${FLOWSIGHT_SKIP_LLM_RUN:-0}" != "1" ]]; then
      "$OUT_DIR/llama-server" --version 2>/dev/null || true
    fi
    return 0
  fi
  echo "[FlowSight] Existing llama-server wrong arch for ${ASSET_ARCH}; ignoring committed binary"
  return 1
}

echo "[FlowSight] Looking up latest llama.cpp release asset for ${ASSET_ARCH}..."
# Prefer rolling bXXXX releases that ship bin-macos-{arm64,x64}. GitHub's
# /releases/latest can point at a versioned tag (e.g. v0.3.0) with no macOS
# assets, which used to force a broken host-native source rebuild for x64 CI.
ASSET_URL=""
ASSET_TAG=""
for page in 1 2 3; do
  API="https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=30&page=${page}"
  if ! github_curl "$API" "$TMP/releases-${page}.json"; then
    echo "[FlowSight] GitHub API lookup failed (page ${page}; rate limit/403?). Will try existing binary or source build."
    break
  fi
  # Exact asset name segment: llama-*-bin-macos-arm64.tar.gz / macos-x64
  MATCH="$(
    python3 - "$TMP/releases-${page}.json" "$ASSET_ARCH" <<'PY'
import json, sys
path, arch = sys.argv[1], sys.argv[2]
needle = f"bin-{arch}"
with open(path, encoding="utf-8") as f:
    releases = json.load(f)
if not isinstance(releases, list):
    sys.exit(0)
for rel in releases:
    for asset in rel.get("assets") or []:
        name = asset.get("name") or ""
        url = asset.get("browser_download_url") or ""
        if needle in name and url:
            print(f"{rel.get('tag_name','')}\t{url}")
            sys.exit(0)
PY
  )" || true
  if [[ -n "${MATCH:-}" ]]; then
    ASSET_TAG="${MATCH%%$'\t'*}"
    ASSET_URL="${MATCH#*$'\t'}"
    echo "[FlowSight] Found ${ASSET_ARCH} asset on release ${ASSET_TAG}"
    break
  fi
done
if [[ -z "${ASSET_URL:-}" ]]; then
  echo "[FlowSight] No ${ASSET_ARCH} prebuilt asset in recent releases. Will try existing binary or source build."
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
  # -a keeps upstream symlinks as symlinks. Plain `cp` dereferences them, which
  # produced three byte-identical copies of every dylib (libggml-base.dylib,
  # .0.dylib and .0.20.1.dylib) — 3x payload, codesign calls and notary scanning.
  /bin/cp -a "$SERVER_DIR"/*.dylib "$OUT_DIR/" 2>/dev/null || true
  /bin/cp -a "$SERVER_DIR/../lib"/*.dylib "$OUT_DIR/" 2>/dev/null || true
  prune_unreferenced_dylibs
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
# Prefer a known-good release tag when HEAD main may not cross-compile cleanly.
CLONE_REF="${FLOWSIGHT_LLAMA_REF:-b10666}"
/usr/bin/git clone --depth 1 --branch "$CLONE_REF" https://github.com/ggml-org/llama.cpp.git "$TMP/llama.cpp" \
  || /usr/bin/git clone --depth 1 https://github.com/ggml-org/llama.cpp.git "$TMP/llama.cpp"
# GGML_NATIVE=OFF avoids host -march=apple-m1 when cross-building x86_64 on arm64 runners.
cmake -S "$TMP/llama.cpp" -B "$TMP/build" \
  -DGGML_METAL=ON \
  -DGGML_NATIVE=OFF \
  -DLLAMA_BUILD_SERVER=ON \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_ARCHITECTURES="$CMAKE_OSX_ARCH"
cmake --build "$TMP/build" --config Release -j "$(sysctl -n hw.ncpu)" --target llama-server
/bin/cp -f "$TMP/build/bin/llama-server" "$OUT_DIR/llama-server"
/bin/chmod +x "$OUT_DIR/llama-server"
/bin/cp -a "$TMP/build/bin"/*.dylib "$OUT_DIR/" 2>/dev/null || true
prune_unreferenced_dylibs
echo "[FlowSight] Built $OUT_DIR/llama-server ($ASSET_ARCH)"
verify_llama_arch "$OUT_DIR/llama-server" "$WANT_ARCH"
