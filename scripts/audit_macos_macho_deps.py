#!/usr/bin/env python3
"""Fail the build when a bundled macOS Mach-O links a library that will not load.

FlowSight ships `llama-server` plus its ggml/llama dylibs inside the .app. A
dependency that is missing at runtime is fatal and unrecoverable: dyld aborts
the process before `main()`, so the app can only report "llama-server exited
during startup".

This is static analysis (`otool -l`), so it also works for the cross-compiled
x86_64 slice that cannot be executed on an arm64 runner.

Three classes of breakage are detected:

1. Absolute system paths (`/usr/lib/...`, `/System/...`) that do not resolve on
   this machine. Note that most system dylibs no longer exist on disk since
   macOS 11 -- they live in the dyld shared cache -- so existence is probed with
   `dlopen`, which consults the cache exactly like dyld does at launch.

   This is the check that would have caught the v3.6.5 release blocker:
   upstream's rolling `bin-macos-arm64` asset is built on a macOS 26 runner
   where CMake auto-detects `/usr/lib/librdma.dylib` and links it into
   `libggml-rpc` as a hard `LC_LOAD_DYLIB`. That library does not exist on
   macOS 15 or earlier, so every user below macOS 26 got SIGABRT.

2. Absolute *non-system* paths (`/opt/homebrew/...`, `/usr/local/...`, a
   CMake build tree). These resolve on CI (`dlopen` / `Path.exists` succeed)
   and abort on every user machine that does not have the same cellar.
   That is how an arm64 Homebrew `libssl` would have shipped from macos-14.

3. `@rpath`/`@loader_path`/`@executable_path` dependencies with no matching file
   in the audited directory, i.e. a dylib that was dropped from the bundle while
   something still links it.

   Only `@loader_path`/`@executable_path`-relative `LC_RPATH` entries count when
   resolving. An absolute `LC_RPATH` (CMake bakes the build tree in by default)
   resolves on the build machine and nowhere else, so trusting it would hide
   exactly the kind of breakage this script exists to find.

`LC_LOAD_WEAK_DYLIB` entries are reported but never fatal: dyld binds those to
NULL when absent instead of aborting.

Usage:
    python3 scripts/audit_macos_macho_deps.py <directory> [more directories...]
"""

from __future__ import annotations

import ctypes
import subprocess
import sys
from functools import lru_cache
from pathlib import Path

OTOOL = "/usr/bin/otool"
FILE = "/usr/bin/file"

# Dependency load commands we care about. LC_ID_DYLIB is deliberately absent:
# it is the library's own install name, not something it loads.
HARD_LOAD_COMMANDS = {"LC_LOAD_DYLIB", "LC_REEXPORT_DYLIB", "LC_LOAD_UPWARD_DYLIB"}
WEAK_LOAD_COMMANDS = {"LC_LOAD_WEAK_DYLIB"}

DYLD_PLACEHOLDERS = ("@rpath", "@loader_path", "@executable_path")

# Only Apple-shipped locations are portable. Homebrew/MacPorts/build-tree
# absolute LC_LOAD_DYLIB entries exist on GitHub-hosted macos-14 runners and
# nowhere on a stock user Mac.
APPLE_SYSTEM_PREFIXES = ("/usr/lib/", "/System/", "/Library/Apple/")


def is_apple_system_dep(dep: str) -> bool:
    return dep.startswith(APPLE_SYSTEM_PREFIXES)


def is_mach_o(path: Path) -> bool:
    if path.is_symlink() or not path.is_file():
        return False
    try:
        described = subprocess.run(
            [FILE, "-b", str(path)], capture_output=True, text=True, check=True
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return False
    return "Mach-O" in described


def parse_load_commands(path: Path) -> tuple[list[tuple[str, str]], list[str]]:
    """Return ([(load_command, dependency_path)], [LC_RPATH paths]) for `path`.

    `otool -l` repeats its output per architecture on fat binaries, so results
    are de-duplicated while preserving order.
    """
    dump = subprocess.run(
        [OTOOL, "-l", str(path)], capture_output=True, text=True, check=True
    ).stdout

    deps: list[tuple[str, str]] = []
    rpaths: list[str] = []
    current_cmd: str | None = None

    for raw in dump.splitlines():
        line = raw.strip()
        if line.startswith("cmd "):
            current_cmd = line.split(None, 1)[1]
        elif line.startswith("name ") and current_cmd in (
            HARD_LOAD_COMMANDS | WEAK_LOAD_COMMANDS
        ):
            # Format: `name /usr/lib/libc++.1.dylib (offset 24)`
            value = line[len("name ") :].rsplit(" (offset ", 1)[0].strip()
            entry = (current_cmd, value)
            if entry not in deps:
                deps.append(entry)
        elif line.startswith("path ") and current_cmd == "LC_RPATH":
            value = line[len("path ") :].rsplit(" (offset ", 1)[0].strip()
            if value not in rpaths:
                rpaths.append(value)

    return deps, rpaths


@lru_cache(maxsize=None)
def system_library_loadable(dep: str) -> bool:
    """True when dyld can resolve `dep`, on disk or from the shared cache."""
    if Path(dep).exists():
        return True
    try:
        ctypes.CDLL(dep)
    except OSError:
        return False
    return True


def bundle_relative_rpaths(macho: Path, rpaths: list[str]) -> list[Path]:
    """LC_RPATH entries that will still resolve after the app is installed."""
    loader_dir = macho.parent
    resolved: list[Path] = []
    for rpath in rpaths:
        if not rpath.startswith(("@loader_path", "@executable_path")):
            continue
        tail = rpath.split("/", 1)[1] if "/" in rpath else ""
        resolved.append(loader_dir / tail if tail else loader_dir)
    return resolved


def resolve_placeholder(dep: str, macho: Path, rpaths: list[str]) -> bool:
    """True when an @rpath/@loader_path dependency has a file behind it."""
    suffix = dep.split("/", 1)[1] if "/" in dep else ""

    if dep.startswith("@rpath"):
        search_dirs = bundle_relative_rpaths(macho, rpaths)
    else:
        # @loader_path / @executable_path are already relative to the bundle.
        search_dirs = [macho.parent]

    return any((directory / suffix).exists() for directory in search_dirs)


def audit_directory(directory: Path) -> tuple[int, list[str]]:
    """Audit every Mach-O under `directory`. Returns (files_checked, errors)."""
    errors: list[str] = []
    checked = 0

    for path in sorted(directory.rglob("*")):
        if not is_mach_o(path):
            continue
        checked += 1
        deps, rpaths = parse_load_commands(path)

        # Reported once per file: without it every @rpath dependency would be
        # listed as missing, which buries the actual cause.
        needs_rpath = any(dep.startswith("@rpath") for _, dep in deps)
        rpath_is_portable = bool(bundle_relative_rpaths(path, rpaths))
        if needs_rpath and not rpath_is_portable:
            errors.append(
                f"{path.name}: has @rpath dependencies but no "
                f"@loader_path-relative LC_RPATH (found: {rpaths or 'none'}). "
                f"Those dylibs can only be found on the build machine. Build with "
                f"-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON -DCMAKE_INSTALL_RPATH=@loader_path"
            )

        for load_command, dep in deps:
            weak = load_command in WEAK_LOAD_COMMANDS

            if dep.startswith("@rpath") and not rpath_is_portable:
                continue

            if dep.startswith(DYLD_PLACEHOLDERS):
                if resolve_placeholder(dep, path, rpaths):
                    continue
                message = (
                    f"{path.name}: links {dep} ({load_command}) but no such file "
                    f"is bundled in {directory}"
                )
            elif dep.startswith("/"):
                if not is_apple_system_dep(dep):
                    message = (
                        f"{path.name}: links non-system library {dep} "
                        f"({load_command}). Homebrew/MacPorts/build-tree paths "
                        f"resolve on CI and abort on user machines"
                    )
                elif system_library_loadable(dep):
                    continue
                else:
                    message = (
                        f"{path.name}: links absolute system library {dep} "
                        f"({load_command}) which does not exist on this machine and "
                        f"is not in the dyld shared cache"
                    )
            else:
                continue

            if weak:
                print(f"[audit]   warning (weak, non-fatal): {message}")
            else:
                errors.append(message)

    return checked, errors


def main(argv: list[str]) -> int:
    directories = [Path(arg).resolve() for arg in argv[1:]]
    if not directories:
        print(f"usage: {Path(argv[0]).name} <directory> [more directories...]")
        return 2

    total_checked = 0
    all_errors: list[str] = []

    for directory in directories:
        if not directory.is_dir():
            print(f"[audit] ERROR: not a directory: {directory}")
            return 1
        print(f"[audit] Auditing Mach-O dependencies under {directory}")
        checked, errors = audit_directory(directory)
        print(f"[audit]   {checked} Mach-O file(s) inspected")
        total_checked += checked
        all_errors.extend(errors)

    if total_checked == 0:
        print("[audit] ERROR: no Mach-O files found — nothing was actually audited")
        return 1

    if all_errors:
        print(f"\n[audit] FAILED: {len(all_errors)} unresolvable dependency reference(s):")
        for error in all_errors:
            print(f"[audit]   - {error}")
        print(
            "\n[audit] These binaries would abort at dyld load time on a machine "
            "without those libraries. Do not ship them."
        )
        return 1

    print(f"[audit] OK: all {total_checked} Mach-O file(s) have resolvable dependencies")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
