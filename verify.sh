#!/usr/bin/env bash
#
# Acceptance script for `ruling`.
#
# Exercises end-to-end checks including a real `diff -u` parity comparison
# against the system diff when available. Exits 0 only when every check passes.
set -u

cd "$(dirname "$0")"

PASS=0
FAIL=0

check() {
    local name="$1"
    local result="$2"
    if [ "$result" -eq 0 ]; then
        echo "PASS: $name"
        PASS=$((PASS + 1))
    else
        echo "FAIL: $name"
        FAIL=$((FAIL + 1))
    fi
}

# Build and run unit + integration tests.
cargo build --quiet 2>/dev/null
check "cargo build" $?
cargo test --quiet 2>/dev/null
check "cargo test" $?

# Create a scratch directory for file fixtures.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

printf 'alpha\nbeta\ngamma\n' > "$TMP/identical_a.txt"
printf 'alpha\nbeta\ngamma\n' > "$TMP/identical_b.txt"
printf 'alpha\nCHANGED\ngamma\n' > "$TMP/diff_a.txt"
printf 'alpha\nchanged\ngamma\n' > "$TMP/diff_b.txt"
printf '\xff\xfe\x00\x01' > "$TMP/bin_a.bin"
printf '\x00\x01\x02\x03' > "$TMP/bin_b.bin"

# 1. Identical files -> exit 0, no output.
./target/debug/ruling "$TMP/identical_a.txt" "$TMP/identical_b.txt" > "$TMP/out" 2>&1
code=$?
check "identical files exit 0" $([ "$code" -eq 0 ] && [ ! -s "$TMP/out" ]; echo $?)

# 2. Different files -> exit 1.
./target/debug/ruling "$TMP/diff_a.txt" "$TMP/diff_b.txt" > /dev/null 2>&1
check "different files exit 1" $([ $? -eq 1 ]; echo $?)

# 3. Missing file -> exit 2.
./target/debug/ruling "$TMP/does_not_exist.txt" "$TMP/diff_b.txt" > /dev/null 2>&1
check "missing file exit 2" $([ $? -eq 2 ]; echo $?)

# 4. Brief mode -> exit 1 and prints 'Files ... differ'.
out=$(./target/debug/ruling -q "$TMP/diff_a.txt" "$TMP/diff_b.txt" 2>&1)
code=$?
check "brief mode" $([ "$code" -eq 1 ] && printf '%s' "$out" | grep -q "differ"; echo $?)

# 5. Binary files -> exit 1 and prints 'Binary files ... differ'.
out=$(./target/debug/ruling "$TMP/bin_a.bin" "$TMP/bin_b.bin" 2>&1)
code=$?
check "binary files" $([ "$code" -eq 1 ] && printf '%s' "$out" | grep -q "Binary files"; echo $?)

# 6. Ignore-case: differing case treated as identical.
./target/debug/ruling -i "$TMP/diff_a.txt" "$TMP/diff_b.txt" > /dev/null 2>&1
check "ignore-case" $([ $? -eq 0 ]; echo $?)

# 7. Unified diff parity vs system diff -u (when available).
if command -v diff > /dev/null 2>&1; then
    ./target/debug/ruling "$TMP/diff_a.txt" "$TMP/diff_b.txt" > "$TMP/ours.txt" 2>&1
    diff -u "$TMP/diff_a.txt" "$TMP/diff_b.txt" > "$TMP/system.txt" 2>&1
    # Compare the hunk/body portions. The header lines differ (we print plain
    # paths, GNU diff prints timestamps), so compare everything after the
    # first two header lines.
    ours=$(tail -n +3 "$TMP/ours.txt")
    sys=$(tail -n +3 "$TMP/system.txt")
    check "diff -u parity" $([ "$ours" = "$sys" ]; echo $?)
else
    echo "SKIP: diff -u parity (system diff not available)"
fi

echo
echo "Passed: $PASS, Failed: $FAIL"
[ "$FAIL" -eq 0 ]
