#!/usr/bin/env bash
# Produce llama-server + its runtime dylibs for macOS into local_llm/bin/.
#
# ## Why this builds from source instead of downloading upstream's asset
#
# v3.6.5 shipped a bundled llama-server that aborted (SIGABRT) at launch on
# macOS 15 and earlier:
#
#   dyld: Library not loaded: /usr/lib/librdma.dylib
#     Referenced from: .../local_llm/bin/libggml-rpc.0.dylib
#
# This script used to download the *latest* `bin-macos-{arm64,x64}` asset from
# ggml-org/llama.cpp with no version pin. Upstream builds the arm64 asset on a
# macOS 26 runner, where ggml's RPC backend auto-detects `/usr/lib/librdma.dylib`
# (Apple RDMA-over-Thunderbolt) and links it as a hard LC_LOAD_DYLIB. That
# library does not exist below macOS 26 and is not in the dyld shared cache, and
# llama-server hard-links libggml-rpc, so the load failure is unavoidable.
#
# Building from a pinned tag with `-DGGML_RPC=OFF` removes the failure mode at
# the root: the RPC backend is never compiled, so no librdma reference can exist.
# FlowSight runs inference in-process against localhost and never uses ggml's
# distributed/RPC backend, so nothing is lost.
#
# The build is also pinned and self-contained on purpose:
#   * GGML_METAL_EMBED_LIBRARY=ON     shaders live inside libggml-metal, so there
#                                     is no default.metallib to forget to copy
#   * CMAKE_INSTALL_RPATH=@loader_path binaries find their dylibs next to
#                                     themselves inside the .app, not in the
#                                     build tree
#   * CMAKE_OSX_DEPLOYMENT_TARGET     taken from tauri.conf.json, so the runtime
#                                     cannot silently require a newer macOS than
#                                     the app claims to support
#   * GGML_BLAS=OFF                   ggml's Accelerate backend calls
#                                     cblas_sgemm, which Apple only exposes from
#                                     macOS 13.3. Below that it links as a weak
#                                     external that resolves to NULL, i.e. a
#                                     null-call crash the moment the backend runs.
#                                     Metal does the offload and the CPU backend
#                                     keeps its NEON kernels, so this only gives
#                                     up large-batch CPU prompt throughput.
#   * -Werror=unguarded-availability-new
#                                     any future upstream call into an API newer
#                                     than the deployment target fails the build
#                                     instead of shipping another weak NULL
#   * LLAMA_OPENSSL=OFF               llama.cpp defaults this ON and then
#                                     find_package(OpenSSL) picks Homebrew's
#                                     arm64 libssl on GitHub macos-14 runners.
#                                     Cross-compiling x86_64 then fails at link
#                                     ("ignoring file ... architecture 'arm64'").
#                                     Native arm64 would *succeed* and ship an
#                                     LC_LOAD_DYLIB into /opt/homebrew, which
#                                     exists on CI and nowhere on user Macs.
#                                     FlowSight only talks HTTP to localhost and
#                                     downloads GGUF itself, so HTTPS in
#                                     llama-server is unused.
#   * LLAMA_*_UI = OFF                the server's web UI is unused.
#
# Every path ends in verify_output(), which refuses to leave behind a tree that
# is the wrong architecture, has an unresolvable dependency, or cannot execute.
#
# Env:
#   FLOWSIGHT_LLM_ARCH=macos-arm64|macos-x64  override host arch detection
#   FLOWSIGHT_LLAMA_REF=bXXXXX                llama.cpp tag to build (pinned below)
#   FLOWSIGHT_LLAMA_PREBUILT=1                download upstream's asset for that
#                                             same tag instead of building; still
#                                             fully audited, so an asset carrying
#                                             the librdma link will fail the build
#   FLOWSIGHT_LLAMA_REUSE=1                   reuse an existing local_llm/bin if
#                                             it passes verification (local dev)
#   FLOWSIGHT_SKIP_LLM_RUN=1                  skip the execution smoke test; only
#                                             honoured when the target arch
#                                             genuinely cannot run on this host
#   GITHUB_TOKEN / GH_TOKEN                   GitHub API auth for asset lookup
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/local_llm/bin"
TAURI_CONF="$ROOT/apps/agent/src-tauri/tauri.conf.json"
AUDIT_SCRIPT="$ROOT/scripts/audit_macos_macho_deps.py"
mkdir -p "$OUT_DIR"

# Pinned llama.cpp release. Bump deliberately, and only after re-running this
# script locally: an unpinned "latest" is what shipped the v3.6.5 crash.
LLAMA_REF="${FLOWSIGHT_LLAMA_REF:-b10666}"

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

# Keep the llama runtime's minimum macOS in step with what the app advertises.
DEPLOYMENT_TARGET="$(
  python3 - "$TAURI_CONF" <<'PY'
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8") as f:
        cfg = json.load(f)
    print(cfg["bundle"]["macOS"]["minimumSystemVersion"])
except Exception:
    print("12.0")
PY
)"

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

is_mach_o() {
  /usr/bin/file -b "$1" 2>/dev/null | /usr/bin/grep -q 'Mach-O'
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
  return 0
}

# A single wrong-arch dylib is enough to break the bundle at load time, so check
# the whole tree rather than just llama-server.
verify_all_arch() {
  local failed=0
  local bin
  while IFS= read -r -d '' bin; do
    if ! is_mach_o "$bin"; then
      continue
    fi
    if ! verify_llama_arch "$bin" "$WANT_ARCH"; then
      failed=1
    fi
  done < <(/usr/bin/find "$OUT_DIR" -type f -print0)
  if [[ "$failed" -ne 0 ]]; then
    echo "[FlowSight] ERROR: local_llm/bin contains Mach-O files that are not ${WANT_ARCH}"
    return 1
  fi
  echo "[FlowSight] Arch OK: every bundled Mach-O is ${WANT_ARCH}"
  return 0
}

# True when a $WANT_ARCH binary can actually be executed on this host.
# Rosetta is intentionally not treated as "can execute": v3.6.7's x86_64
# llama-server built, audited, then died with SIGILL (exit 132) under
# `arch -x86_64` on macos-14. `arch -x86_64 true` succeeding does not mean
# ggml's CPU backend will start. Intel Macs run the binary natively; the
# static Mach-O audit still covers the cross-compiled slice.
can_execute_target() {
  local host
  host="$(uname -m)"
  if [[ "$WANT_ARCH" == "$host" ]]; then
    return 0
  fi
  return 1
}

# The old version of this check was `llama-server --version 2>/dev/null || true`,
# which threw away both stderr and the exit code — the reason CI shipped a binary
# that could not start. It now fails the build, and FLOWSIGHT_SKIP_LLM_RUN is
# refused when the binary runs natively here (Apple Silicon job).
smoke_test() {
  if ! can_execute_target; then
    echo "[FlowSight] Skipping execution smoke test: ${WANT_ARCH} cannot run natively on $(uname -m)"
    echo "[FlowSight] (the dependency audit above is static and still covers this build)"
    return 0
  fi
  if [[ "${FLOWSIGHT_SKIP_LLM_RUN:-0}" == "1" ]]; then
    echo "[FlowSight] Ignoring FLOWSIGHT_SKIP_LLM_RUN=1: ${WANT_ARCH} runs natively here," \
      "so llama-server must prove it starts"
  fi
  echo "[FlowSight] Running llama-server --version"
  local status=0
  "$OUT_DIR/llama-server" --version > "$TMP/version.log" 2>&1 || status=$?
  if [[ "$status" -ne 0 ]]; then
    echo "[FlowSight] ERROR: llama-server --version failed (exit ${status}). Output:"
    /bin/cat "$TMP/version.log"
    return 1
  fi
  /bin/cat "$TMP/version.log"
  return 0
}

dependency_audit() {
  echo "[FlowSight] Auditing bundled Mach-O dependencies"
  python3 "$AUDIT_SCRIPT" "$OUT_DIR"
}

verify_output() {
  test -x "$OUT_DIR/llama-server"
  verify_all_arch
  dependency_audit
  smoke_test
  echo "[FlowSight] local_llm/bin verified for ${ASSET_ARCH} (llama.cpp ${LLAMA_REF})"
}

clear_out_dir() {
  /usr/bin/find "$OUT_DIR" -maxdepth 1 \( -name 'llama-server' -o -name '*.dylib' \) -delete 2>/dev/null || true
}

# llama.cpp releases ship every CLI tool's helper dylib plus three byte-identical
# aliases of each library (libggml-base.dylib / .0.dylib / .0.20.1.dylib). Tauri
# copies resources with fs::copy, which dereferences symlinks, so the only way to
# keep the bundle small is to ship just the libraries llama-server actually loads.
# Everything is reachable statically (verified with otool: the ggml backends are
# LC_LOAD_DYLIB entries, not dlopen'd), so the link closure is exact.
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

install_prebuilt() {
  echo "[FlowSight] Resolving ${ASSET_ARCH} asset for pinned llama.cpp ${LLAMA_REF}..."
  local api="https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/${LLAMA_REF}"
  if ! github_curl "$api" "$TMP/release.json"; then
    echo "[FlowSight] ERROR: could not read release ${LLAMA_REF} from the GitHub API"
    return 1
  fi
  local url
  url="$(
    python3 - "$TMP/release.json" "$ASSET_ARCH" <<'PY'
import json, sys
path, arch = sys.argv[1], sys.argv[2]
needle = f"bin-{arch}"
with open(path, encoding="utf-8") as f:
    release = json.load(f)
for asset in release.get("assets") or []:
    if needle in (asset.get("name") or "") and asset.get("browser_download_url"):
        print(asset["browser_download_url"])
        break
PY
  )"
  if [[ -z "$url" ]]; then
    echo "[FlowSight] ERROR: release ${LLAMA_REF} has no bin-${ASSET_ARCH} asset"
    return 1
  fi

  echo "[FlowSight] Downloading $url"
  local ext="bin"
  case "$url" in
    *.tar.gz) ext="tar.gz" ;;
    *.tgz) ext="tgz" ;;
    *.zip) ext="zip" ;;
  esac
  local archive="$TMP/llama.$ext"
  /usr/bin/curl -fL "$url" -o "$archive"

  extract_archive "$archive" "$TMP/extract"
  local server
  server="$(/usr/bin/find "$TMP/extract" -type f -name 'llama-server' | /usr/bin/head -n1 || true)"
  if [[ -z "$server" ]]; then
    echo "[FlowSight] ERROR: archive had no llama-server"
    return 1
  fi

  # Only wipe previous bins once we have a usable download in hand.
  clear_out_dir
  local server_dir
  server_dir="$(/usr/bin/dirname "$server")"
  /bin/cp -f "$server" "$OUT_DIR/llama-server"
  /bin/chmod +x "$OUT_DIR/llama-server"
  # -a keeps upstream symlinks as symlinks. Plain `cp` dereferences them, which
  # produced three byte-identical copies of every dylib (libggml-base.dylib,
  # .0.dylib and .0.20.1.dylib) — 3x payload, codesign calls and notary scanning.
  /bin/cp -a "$server_dir"/*.dylib "$OUT_DIR/" 2>/dev/null || true
  /bin/cp -a "$server_dir/../lib"/*.dylib "$OUT_DIR/" 2>/dev/null || true
  prune_unreferenced_dylibs
  echo "[FlowSight] Installed prebuilt $OUT_DIR/llama-server (${ASSET_ARCH}, ${LLAMA_REF})"
}

build_from_source() {
  echo "[FlowSight] Building llama.cpp ${LLAMA_REF} from source for ${ASSET_ARCH}" \
    "(Metal, RPC off, deployment target ${DEPLOYMENT_TARGET})..."
  if ! command -v cmake >/dev/null 2>&1; then
    echo "cmake is required to build llama.cpp. Install with: brew install cmake"
    exit 1
  fi

  # Pinned tag only: falling back to the default branch would reintroduce the
  # unpinned-provenance bug this script exists to prevent.
  /usr/bin/git clone --depth 1 --branch "$LLAMA_REF" \
    https://github.com/ggml-org/llama.cpp.git "$TMP/llama.cpp"

  # Reported by `llama-server --version`, so support tickets name the exact
  # upstream build we shipped. A shallow clone has no history to count.
  # A scalar rather than an array: macOS still ships bash 3.2, where expanding an
  # empty array under `set -u` is an unbound-variable error.
  local build_number="${LLAMA_REF#b}"
  local build_number_flag=""
  if [[ "$build_number" =~ ^[0-9]+$ ]]; then
    build_number_flag="-DLLAMA_BUILD_NUMBER=$build_number"
  fi

  # GGML_NATIVE=OFF avoids host -march=apple-m1 when cross-building x86_64 on
  # arm64 runners.
  local availability_flag="-Werror=unguarded-availability-new"
  cmake -S "$TMP/llama.cpp" -B "$TMP/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DGGML_METAL=ON \
    -DGGML_METAL_EMBED_LIBRARY=ON \
    -DGGML_RPC=OFF \
    -DGGML_BLAS=OFF \
    -DGGML_NATIVE=OFF \
    -DCMAKE_C_FLAGS="$availability_flag" \
    -DCMAKE_CXX_FLAGS="$availability_flag" \
    -DCMAKE_OBJC_FLAGS="$availability_flag" \
    -DCMAKE_OBJCXX_FLAGS="$availability_flag" \
    -DLLAMA_BUILD_SERVER=ON \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_OPENSSL=OFF \
    -DLLAMA_BUILD_UI=OFF \
    -DLLAMA_USE_PREBUILT_UI=OFF \
    -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON \
    -DCMAKE_INSTALL_RPATH='@loader_path' \
    -DCMAKE_OSX_ARCHITECTURES="$WANT_ARCH" \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
    ${build_number_flag:+"$build_number_flag"}
  cmake --build "$TMP/build" --config Release \
    -j "$(/usr/sbin/sysctl -n hw.ncpu)" --target llama-server

  clear_out_dir
  /bin/cp -f "$TMP/build/bin/llama-server" "$OUT_DIR/llama-server"
  /bin/chmod +x "$OUT_DIR/llama-server"
  /bin/cp -a "$TMP/build/bin"/*.dylib "$OUT_DIR/"
  prune_unreferenced_dylibs
  echo "[FlowSight] Built $OUT_DIR/llama-server (${ASSET_ARCH}, ${LLAMA_REF})"
}

if [[ "${FLOWSIGHT_LLAMA_REUSE:-0}" == "1" ]] && [[ -x "$OUT_DIR/llama-server" ]]; then
  echo "[FlowSight] FLOWSIGHT_LLAMA_REUSE=1 — verifying existing local_llm/bin"
  if verify_output; then
    exit 0
  fi
  echo "[FlowSight] Existing local_llm/bin did not verify; rebuilding"
fi

if [[ "${FLOWSIGHT_LLAMA_PREBUILT:-0}" == "1" ]]; then
  install_prebuilt
else
  build_from_source
fi

verify_output
