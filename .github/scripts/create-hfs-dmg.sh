#!/usr/bin/env bash
# Keep all mounts and working images inside this job's temporary directory.
set -euo pipefail

MODE="${1:?create, probe or validate}"
INPUT="${2:?staging directory, signed app archive or stapled target}"
WORK="$(mktemp -d "${RUNNER_TEMP:?}/flowsight-hfs.XXXXXX")"
MOUNT="$WORK/mount"
mkdir -p "$MOUNT"
MOUNT_ACTIVE=0
WRITABLE_COUNT=0

detach_owned_mount() {
  if [[ "$MOUNT_ACTIVE" -eq 0 ]]; then return 0; fi
  for attempt in 1 2 3 4 5; do
    if hdiutil detach "$MOUNT"; then MOUNT_ACTIVE=0; return 0; fi
    sleep 3
  done
  hdiutil detach -force "$MOUNT"
  MOUNT_ACTIVE=0
}

cleanup() {
  if [[ "$MOUNT_ACTIVE" -eq 1 ]]; then
    if ! hdiutil detach -force "$MOUNT" >/dev/null 2>&1; then
      echo "Owned mount remains active; preserving $WORK" >&2
      return
    fi
    MOUNT_ACTIVE=0
  fi
  # WORK was created under RUNNER_TEMP by this invocation; no external mounts.
  case "$WORK" in "$RUNNER_TEMP"/flowsight-hfs.*) rm -rf "$WORK" ;; esac
}
trap cleanup EXIT

assert_hfs() {
  diskutil info -plist "$MOUNT" > "$WORK/filesystem.plist"
  python3 - "$WORK/filesystem.plist" <<'PY'
import plistlib, sys
info = plistlib.load(open(sys.argv[1], 'rb'))
assert info['FilesystemType'] == 'hfs', info
print('DMG_FILESYSTEM=' + info['FilesystemType'])
PY
}

validate_stapled_ticket() {
  local target="$1" attempt status delay=5
  local log="$WORK/stapler-validate.log"
  for attempt in 1 2 3 4 5; do
    echo "STAPLER_VALIDATE_ATTEMPT=$attempt/5 target=$target"
    if xcrun stapler validate "$target" > "$log" 2>&1; then
      cat "$log"
      echo "STAPLER_TICKET_VALIDATE=success"
      return 0
    else
      status=$?
      cat "$log"
    fi
    # Retry only the observed CloudKit connection loss. An invalid/missing ticket
    # or another validation error must still fail immediately.
    if ! grep -Eq 'NSURLErrorDomain Code=-1005 .*network connection was lost' "$log"; then
      echo "STAPLER_TICKET_VALIDATE=failed status=$status non_transient=true" >&2
      return "$status"
    fi
    if [[ "$attempt" -eq 5 ]]; then
      echo "STAPLER_CLOUDKIT_TRANSPORT_FAILURE=exhausted status=$status" >&2
      date -u
      dscacheutil -q host -a name api.apple-cloudkit.com || true
      curl --head --connect-timeout 10 --max-time 20 https://api.apple-cloudkit.com/ || true
      return "$status"
    fi
    echo "STAPLER_CLOUDKIT_RETRY_DELAY_SECONDS=$delay"
    sleep "$delay"
    delay=$((delay * 2))
  done
}

validate_payload() {
  codesign --verify --deep --strict "$MOUNT/FlowSight Agent.app"
  if [[ "${FLOWSIGHT_DMG_REQUIRE_NOTARY:-1}" -eq 1 ]]; then
    validate_stapled_ticket "$MOUNT/FlowSight Agent.app"
  fi
}

two_step_hfs() {
  local stage="$1" output="$2"
  local kib mib
  WRITABLE_COUNT=$((WRITABLE_COUNT + 1))
  local writable="$WORK/writable-$WRITABLE_COUNT.dmg"
  kib="$(du -sk "$stage" | awk '{print $1}')"
  mib=$(((kib + 1023) / 1024 + 256))
  echo "DMG_METHOD=HFS_UDRW_ATTACH_DITTO_UDZO"
  hdiutil create -size "${mib}m" -volname "FlowSight Agent" -fs HFS+ -type UDIF "$writable"
  hdiutil imageinfo -plist "$writable" > "$WORK/writable-image.plist"
  python3 - "$WORK/writable-image.plist" <<'PY'
import plistlib, sys
info = plistlib.load(open(sys.argv[1], 'rb'))
assert info['Format'] == 'UDRW', info
print('DMG_WRITABLE_FORMAT=' + info['Format'])
PY
  hdiutil attach "$writable" -nobrowse -noautoopen -mountpoint "$MOUNT"
  MOUNT_ACTIVE=1
  assert_hfs
  ditto "$stage/FlowSight Agent.app" "$MOUNT/FlowSight Agent.app"
  ln -s /Applications "$MOUNT/Applications"
  validate_payload
  sync
  detach_owned_mount
  hdiutil convert "$writable" -format UDZO -o "$output"
  hdiutil verify "$output"
}

inspect_final_image() {
  local output="$1"
  hdiutil attach "$output" -readonly -nobrowse -noautoopen -mountpoint "$MOUNT"
  MOUNT_ACTIVE=1
  assert_hfs
  validate_payload
  detach_owned_mount
}

create_image() {
  local stage="$1" output="$2"
  python3 - "$stage" "$output" "$RUNNER_TEMP" "$PWD" <<'PY'
import pathlib, sys
stage, output, temporary, checkout = [pathlib.Path(value).resolve() for value in sys.argv[1:]]
assert stage.is_relative_to(temporary) and stage.is_dir(), stage
assert output.is_relative_to(temporary) or output.is_relative_to(checkout), output
assert output.suffix == '.dmg' and not output.exists(), output
assert (stage / 'FlowSight Agent.app').is_dir()
PY
  echo "DMG_METHOD=HFS_DIRECT"
  if hdiutil create -volname "FlowSight Agent" -srcfolder "$stage" -ov -fs HFS+ -format UDZO "$output"; then
    hdiutil verify "$output"
  else
    echo "DMG_DIRECT_HFS_FAILED; using owned HFS writable volume"
    # The destination was checked above and belongs to this build/probe.
    rm -f "$output"
    two_step_hfs "$stage" "$output"
  fi
  inspect_final_image "$output"
}

if [[ "$MODE" == "probe" ]]; then
  STAGE="$WORK/stage"
  mkdir -p "$STAGE"
  python3 - "$INPUT" "$STAGE" <<'PY'
import pathlib, tarfile, sys
archive_path, destination = map(pathlib.Path, sys.argv[1:])
with tarfile.open(archive_path, 'r:gz') as archive:
    for member in archive.getmembers():
        assert member.name == 'FlowSight Agent.app' or member.name.startswith('FlowSight Agent.app/'), member.name
    archive.extractall(destination, filter='data')
PY
  ln -s /Applications "$STAGE/Applications"
  codesign --verify --deep --strict "$STAGE/FlowSight Agent.app"
  validate_stapled_ticket "$STAGE/FlowSight Agent.app"
  echo "FILESYSTEM_PROBE_INPUT=verified_signed_Intel_app; no native runtime execution"
  sw_vers
  uname -m
  if hdiutil create -volname "FlowSight FS probe" -srcfolder "$STAGE" -ov -format UDZO "$WORK/default.dmg"; then
    echo "DMG_DEFAULT_PROBE=success"
  else
    echo "DMG_DEFAULT_PROBE=failed"
  fi
  create_image "$STAGE" "$WORK/hfs-direct.dmg"
  # Exercise the fallback too, avoiding another full compile to discover failure.
  two_step_hfs "$STAGE" "$WORK/hfs-two-step.dmg"
  inspect_final_image "$WORK/hfs-two-step.dmg"
  echo "DMG_HFS_PROBE=success"
elif [[ "$MODE" == "create" ]]; then
  # A normal local/ad-hoc pipeline still checks codesign; signed releases also
  # require the copied payload's notarization ticket, as does every probe.
  [[ "${FLOWSIGHT_DMG_REQUIRE_NOTARY:-1}" =~ ^[01]$ ]]
  create_image "$INPUT" "${3:?DMG output path}"
elif [[ "$MODE" == "validate" ]]; then
  validate_stapled_ticket "$INPUT"
else
  echo "Unknown mode: $MODE" >&2
  exit 2
fi
