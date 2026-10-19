#!/bin/sh
# Hermetic recipe-exit contract for surmount-deploy-host.
# Does not switch a host. Fake ssh only. No cargo, no rustc, no nix build.
#
# Case changed: nixos-rebuild exit 1, smoke exit 0, booted generation
# moves to the generation the switch built -> recipe exit 0.
# Case unchanged: same exits, booted generation stays -> recipe exit 1,
# and the nixos-rebuild error text stays visible.
#
# Exit 2 means the binary was missing and the assertions did not run.

set -eu

bin=${SURMOUNT_DEPLOY_HOST_BIN:-}
if [ -z "$bin" ]; then
  bin=$(command -v surmount-deploy-host 2>/dev/null || true)
fi
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "red execution was not run: surmount-deploy-host binary not found (set SURMOUNT_DEPLOY_HOST_BIN). laptop cargo/rustc and a nix build were not used." >&2
  exit 2
fi

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
state="$root/state"
mkdir -p "$state"
log="$root/ssh.log"

cat > "$root/fake-ssh" << 'EOF'
#!/bin/sh
log="$SURMOUNT_TEST_SSH_LOG"
state="$SURMOUNT_TEST_GENERATION_STATE"
printf '%s\n' "$*" >> "$log"
last=""
for a in "$@"; do
  last=$a
done
case "$last" in
  *nixos-rebuild*)
    echo "synthetic-rebuild-error: switch exit 1" >&2
    if [ "${SURMOUNT_TEST_GENERATION_CHANGED:-0}" = "1" ]; then
      printf '%s\n' 67 > "$state/booted"
      printf '%s\n' 67 > "$state/built"
      echo "synthetic-built-generation: 67"
      echo "synthetic-booted-generation: 67"
    else
      printf '%s\n' 66 > "$state/booted"
      printf '%s\n' unchanged > "$state/built"
      echo "synthetic-built-generation: unchanged"
      echo "synthetic-booted-generation: 66"
    fi
    exit "${SURMOUNT_TEST_REBUILD_EXIT:-1}"
    ;;
  *post-switch-smoke*|*surmount-deploy-host-post-switch-smoke*)
    exit "${SURMOUNT_TEST_SMOKE_EXIT:-0}"
    ;;
  *readlink*|*surmount-test-booted-generation*)
    cat "$state/booted"
    exit 0
    ;;
  *)
    exit "${SURMOUNT_TEST_OTHER_EXIT:-0}"
    ;;
esac
EOF
chmod 755 "$root/fake-ssh"

# The binary reads SURMOUNT_DEPLOY_SSH. Export per case.
run_one() {
  changed=$1
  label=$2
  printf '%s\n' 66 > "$state/booted"
  printf '%s\n' unchanged > "$state/built"
  : > "$log"
  set +e
  out=$(
    SURMOUNT_DEPLOY_SSH="$root/fake-ssh" \
    SURMOUNT_TEST_SSH_LOG="$log" \
    SURMOUNT_TEST_GENERATION_STATE="$state" \
    SURMOUNT_TEST_GENERATION_CHANGED="$changed" \
    SURMOUNT_TEST_REBUILD_EXIT=1 \
    SURMOUNT_TEST_SMOKE_EXIT=0 \
    "$bin" --target example.test --skip-sync --skip-host-local-check 2>&1
  )
  rc=$?
  set -e
  printf '%s\n' "$out" > "$root/$label.out"
  echo "$rc" > "$root/$label.rc"
}

run_one 1 changed
run_one 0 unchanged

changed_rc=$(cat "$root/changed.rc")
unchanged_rc=$(cat "$root/unchanged.rc")
changed_out=$(cat "$root/changed.out")
unchanged_out=$(cat "$root/unchanged.out")
booted_unchanged=$(cat "$state/booted")

fail=0
if [ "$changed_rc" -ne 0 ]; then
  echo "FAIL smoke_exit_0_and_changed_generation_is_recipe_exit_0: expected recipe exit 0, got $changed_rc" >&2
  printf '%s\n' "$changed_out" >&2
  fail=1
else
  echo "PASS smoke_exit_0_and_changed_generation_is_recipe_exit_0"
fi

if [ "$unchanged_rc" -eq 0 ]; then
  echo "FAIL smoke_exit_0_and_unchanged_generation_stays_exit_1_with_rebuild_error: expected recipe exit 1, got 0" >&2
  fail=1
fi
printf '%s\n' "$unchanged_out" | command grep -F "nixos-rebuild failed" >/dev/null || {
  echo "FAIL unchanged case: nixos-rebuild failed text missing" >&2
  fail=1
}
printf '%s\n' "$unchanged_out" | command grep -F "synthetic-rebuild-error: switch exit 1" >/dev/null || {
  echo "FAIL unchanged case: synthetic-rebuild-error swallowed" >&2
  fail=1
}
printf '%s\n' "$booted_unchanged" | command grep -F "66" >/dev/null || {
  echo "FAIL unchanged fixture did not stay on generation 66" >&2
  fail=1
}

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "PASS smoke_exit_0_and_unchanged_generation_stays_exit_1_with_rebuild_error"
exit 0
