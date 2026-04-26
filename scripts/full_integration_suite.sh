#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$REPO_ROOT/target/debug/dari"
TEST_ROOT="${DARI_TEST_ROOT:-/tmp/dari_test}"
RUN_ROOT="${DARI_RUN_ROOT:-/tmp/test_dari}"
LOG_DIR="$TEST_ROOT/logs"

STEP=0

log() {
    printf '\n[%02d] %s\n' "$STEP" "$1"
}

next_step() {
    STEP=$((STEP + 1))
    log "$1"
}

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    exit 1
}

pass() {
    printf 'PASS: %s\n' "$1"
}

run_cmd() {
    local name="$1"
    shift
    local log_file="$LOG_DIR/${STEP}_${name}.log"

    printf '$' >"$log_file"
    printf ' %q' "$@" >>"$log_file"
    printf '\n' >>"$log_file"

    if "$@" >>"$log_file" 2>&1; then
        pass "$name"
    else
        cat "$log_file" >&2
        fail "$name"
    fi
}

run_bash() {
    local name="$1"
    local script_text="$2"
    local log_file="$LOG_DIR/${STEP}_${name}.log"

    printf '%s\n' "$script_text" >"$log_file"

    if bash -lc "set -euo pipefail; $script_text" >>"$log_file" 2>&1; then
        pass "$name"
    else
        cat "$log_file" >&2
        fail "$name"
    fi
}

expect_fail() {
    local name="$1"
    shift
    local log_file="$LOG_DIR/${STEP}_${name}.log"

    printf '$' >"$log_file"
    printf ' %q' "$@" >>"$log_file"
    printf '\n' >>"$log_file"

    if "$@" >>"$log_file" 2>&1; then
        cat "$log_file" >&2
        fail "$name unexpectedly succeeded"
    else
        pass "$name"
    fi
}

assert_file_exists() {
    local path="$1"
    [[ -f "$path" ]] || fail "expected file to exist: $path"
    pass "file exists: $path"
}

assert_dir_exists() {
    local path="$1"
    [[ -d "$path" ]] || fail "expected directory to exist: $path"
    pass "directory exists: $path"
}

assert_not_exists() {
    local path="$1"
    [[ ! -e "$path" ]] || fail "expected path to be absent: $path"
    pass "path absent: $path"
}

assert_contains() {
    local path="$1"
    local needle="$2"
    rg -Fq -- "$needle" "$path" || fail "expected '$needle' in $path"
    pass "found '$needle' in $path"
}

assert_not_contains() {
    local path="$1"
    local needle="$2"
    if rg -Fq -- "$needle" "$path"; then
        fail "did not expect '$needle' in $path"
    fi
    pass "confirmed '$needle' absent from $path"
}

assert_files_equal() {
    local left="$1"
    local right="$2"
    cmp -s "$left" "$right" || fail "files differ: $left vs $right"
    pass "files match: $left == $right"
}

write_png_fixture() {
    local output="$1"
    base64 -d >"$output" <<'EOF'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+lmX8AAAAASUVORK5CYII=
EOF
}

inspect_and_quit() {
    local archive="$1"
    shift
    local log_file
    log_file="$LOG_DIR/${STEP}_inspect_$(basename "$archive").log"

    {
        printf 'inspect %s %s\n' "$archive" "$*"
        printf 'q' | TERM=xterm script -qec "$BIN inspect -f '$archive' $*" /dev/null
    } >"$log_file" 2>&1 || {
        cat "$log_file" >&2
        fail "inspect failed for $archive"
    }

    pass "inspect exited cleanly for $archive"
}

prepare_environment() {
    mkdir -p "$RUN_ROOT"
    rm -rf "$TEST_ROOT"
    mkdir -p "$LOG_DIR"
}

prepare_fixtures() {
    mkdir -p \
        "$TEST_ROOT/fixtures/basic_input/docs" \
        "$TEST_ROOT/fixtures/basic_input/src" \
        "$TEST_ROOT/fixtures/basic_input/dup" \
        "$TEST_ROOT/fixtures/basic_input/images" \
        "$TEST_ROOT/fixtures/basic_input/.git" \
        "$TEST_ROOT/fixtures/append_unique_input/newdir" \
        "$TEST_ROOT/fixtures/append_conflict_rename" \
        "$TEST_ROOT/fixtures/append_conflict_overwrite" \
        "$TEST_ROOT/fixtures/plain_small" \
        "$TEST_ROOT/fixtures/plain_in_place" \
        "$TEST_ROOT/fixtures/chunked_input" \
        "$TEST_ROOT/fixtures/incremental_input" \
        "$TEST_ROOT/fixtures/preserve_input" \
        "$TEST_ROOT/fixtures/v6_input/nested" \
        "$TEST_ROOT/fixtures/split_input/assets"

    cat >"$TEST_ROOT/fixtures/basic_input/.gitignore" <<'EOF'
ignored_git.txt
EOF
    cat >"$TEST_ROOT/fixtures/basic_input/.darignore" <<'EOF'
ignored_dar.txt
EOF
    cat >"$TEST_ROOT/fixtures/basic_input/docs/readme.md" <<'EOF'
# dari integration test

This file should round-trip through create, list, and extract.
EOF
    cat >"$TEST_ROOT/fixtures/basic_input/src/main.rs" <<'EOF'
fn main() {
    println!("integration");
}
EOF
    cat >"$TEST_ROOT/fixtures/basic_input/dup/a.txt" <<'EOF'
duplicate payload
EOF
    cp "$TEST_ROOT/fixtures/basic_input/dup/a.txt" "$TEST_ROOT/fixtures/basic_input/dup/b.txt"
    cat >"$TEST_ROOT/fixtures/basic_input/ignored_git.txt" <<'EOF'
ignored by gitignore
EOF
    cat >"$TEST_ROOT/fixtures/basic_input/ignored_dar.txt" <<'EOF'
ignored by darignore
EOF
    write_png_fixture "$TEST_ROOT/fixtures/basic_input/images/pixel.png"

    cat >"$TEST_ROOT/fixtures/append_unique_input/newdir/after.txt" <<'EOF'
appended unique file
EOF
    cat >"$TEST_ROOT/fixtures/append_conflict_rename/conflict.txt" <<'EOF'
rename payload
EOF
    cat >"$TEST_ROOT/fixtures/append_conflict_overwrite/conflict.txt" <<'EOF'
overwrite payload
EOF
    cat >"$TEST_ROOT/fixtures/plain_small/secret.txt" <<'EOF'
very secret text
EOF
    cat >"$TEST_ROOT/fixtures/plain_in_place/in_place.txt" <<'EOF'
encrypt me in place
EOF
    head -c 1572864 /dev/urandom >"$TEST_ROOT/fixtures/chunked_input/chunked.bin"
    cat >"$TEST_ROOT/fixtures/chunked_input/chunked.txt" <<'EOF'
chunked encryption integration payload
EOF
    cat >"$TEST_ROOT/fixtures/incremental_input/keep.txt" <<'EOF'
original keep payload
EOF
    cat >"$TEST_ROOT/fixtures/incremental_input/change.txt" <<'EOF'
original change payload
EOF
    cat >"$TEST_ROOT/fixtures/preserve_input/base.txt" <<'EOF'
xattr and hardlink payload
EOF
    ln "$TEST_ROOT/fixtures/preserve_input/base.txt" "$TEST_ROOT/fixtures/preserve_input/linked.txt"
    if command -v setfattr >/dev/null 2>&1; then
        setfattr -n user.dari.test -v roundtrip "$TEST_ROOT/fixtures/preserve_input/base.txt"
    elif command -v xattr >/dev/null 2>&1; then
        xattr -w user.dari.test roundtrip "$TEST_ROOT/fixtures/preserve_input/base.txt"
    fi
    cat >"$TEST_ROOT/fixtures/v6_input/nested/data.txt" <<'EOF'
v6 archive payload
EOF
    cat >"$TEST_ROOT/fixtures/v6_input/nested/other.txt" <<'EOF'
second v6 payload
EOF

    head -c 16384 /dev/urandom >"$TEST_ROOT/fixtures/split_input/assets/random.bin"
    cat >"$TEST_ROOT/fixtures/split_input/assets/notes.txt" <<'EOF'
split archive manifest
EOF
}

build_binary() {
    (cd "$REPO_ROOT" && cargo build --quiet)
    [[ -x "$BIN" ]] || fail "binary not found after build: $BIN"
}

main() {
    next_step "Preparing workspace and fixtures"
    prepare_environment
    prepare_fixtures
    build_binary

    next_step "Testing create dry-run and default v6 archive creation"
    run_cmd create_dry_run \
        "$BIN" create -f "$TEST_ROOT/basic/dry-run.dar" --dry-run "$TEST_ROOT/fixtures/basic_input"
    assert_not_exists "$TEST_ROOT/basic/dry-run.dar"
    assert_contains "$LOG_DIR/${STEP}_create_dry_run.log" "docs/readme.md"
    mkdir -p "$TEST_ROOT/basic"
    run_cmd create_default_archive \
        "$BIN" create -f "$TEST_ROOT/basic/basic.dar" --compress-images -v "$TEST_ROOT/fixtures/basic_input"
    assert_file_exists "$TEST_ROOT/basic/basic.dar"
    assert_file_exists "$TEST_ROOT/basic/basic.dari"
    assert_file_exists "$TEST_ROOT/basic/basic.dar.b3"

    next_step "Testing list and extract on default v6 archive"
    run_bash list_table \
        "\"$BIN\" list -f \"$TEST_ROOT/basic/basic.dar\" > \"$TEST_ROOT/basic/list.txt\""
    run_bash list_json \
        "\"$BIN\" list -f \"$TEST_ROOT/basic/basic.dar\" --json > \"$TEST_ROOT/basic/list.json\""
    assert_contains "$TEST_ROOT/basic/list.json" "\"path\": \"docs/readme.md\""
    assert_contains "$TEST_ROOT/basic/list.json" "\"path\": \"src/main.rs\""
    assert_contains "$TEST_ROOT/basic/list.json" "\"linked\": true"
    assert_not_contains "$TEST_ROOT/basic/list.json" "ignored_git.txt"
    assert_not_contains "$TEST_ROOT/basic/list.json" "ignored_dar.txt"
    run_cmd extract_all_v5 \
        "$BIN" extract -f "$TEST_ROOT/basic/basic.dar" -d "$TEST_ROOT/basic/extracted_all"
    assert_files_equal \
        "$TEST_ROOT/fixtures/basic_input/docs/readme.md" \
        "$TEST_ROOT/basic/extracted_all/docs/readme.md"
    assert_files_equal \
        "$TEST_ROOT/fixtures/basic_input/src/main.rs" \
        "$TEST_ROOT/basic/extracted_all/src/main.rs"
    assert_not_exists "$TEST_ROOT/basic/extracted_all/ignored_git.txt"
    assert_not_exists "$TEST_ROOT/basic/extracted_all/ignored_dar.txt"
    run_cmd extract_selective_v5 \
        "$BIN" extract -f "$TEST_ROOT/basic/basic.dar" -d "$TEST_ROOT/basic/extracted_selected" \
        docs/readme.md
    assert_file_exists "$TEST_ROOT/basic/extracted_selected/docs/readme.md"
    assert_not_exists "$TEST_ROOT/basic/extracted_selected/src/main.rs"

    next_step "Testing explicit v5 archive creation for legacy workflows"
    mkdir -p "$TEST_ROOT/legacy_v5"
    run_cmd create_legacy_v5_archive \
        "$BIN" create -f "$TEST_ROOT/legacy_v5/basic_v5.dar" --format-version 5 \
        "$TEST_ROOT/fixtures/basic_input"
    assert_file_exists "$TEST_ROOT/legacy_v5/basic_v5.dar"
    assert_not_exists "$TEST_ROOT/legacy_v5/basic_v5.dari"
    assert_not_exists "$TEST_ROOT/legacy_v5/basic_v5.dar.b3"

    next_step "Testing v5 to v6 migration workflow"
    mkdir -p "$TEST_ROOT/migrate"
    run_cmd migrate_v5_to_v6 \
        "$BIN" migrate -f "$TEST_ROOT/legacy_v5/basic_v5.dar" -o "$TEST_ROOT/migrate/migrated.dar"
    assert_file_exists "$TEST_ROOT/migrate/migrated.dar"
    assert_file_exists "$TEST_ROOT/migrate/migrated.dari"
    assert_file_exists "$TEST_ROOT/migrate/migrated.dar.b3"
    run_bash migrate_list_json \
        "\"$BIN\" list -f \"$TEST_ROOT/migrate/migrated.dar\" --json > \"$TEST_ROOT/migrate/list.json\""
    assert_contains "$TEST_ROOT/migrate/list.json" "\"path\": \"docs/readme.md\""
    assert_contains "$TEST_ROOT/migrate/list.json" "\"path\": \"dup/a.txt\""
    run_cmd verify_migrated_v6 \
        "$BIN" verify -f "$TEST_ROOT/migrate/migrated.dar" --full
    run_cmd extract_migrated_v6 \
        "$BIN" extract -f "$TEST_ROOT/migrate/migrated.dar" -d "$TEST_ROOT/migrate/out"
    assert_files_equal \
        "$TEST_ROOT/fixtures/basic_input/docs/readme.md" \
        "$TEST_ROOT/migrate/out/docs/readme.md"
    assert_files_equal \
        "$TEST_ROOT/fixtures/basic_input/dup/a.txt" \
        "$TEST_ROOT/migrate/out/dup/a.txt"
    expect_fail migrate_v6_rejected \
        "$BIN" migrate -f "$TEST_ROOT/migrate/migrated.dar" -o "$TEST_ROOT/migrate/should_fail.dar"

    next_step "Testing append conflict handling and overwrite behavior"
    mkdir -p "$TEST_ROOT/append"
    mkdir -p "$TEST_ROOT/fixtures/append_base"
    cat >"$TEST_ROOT/fixtures/append_base/conflict.txt" <<'EOF'
original payload
EOF
    run_cmd create_append_base \
        "$BIN" create -f "$TEST_ROOT/append/conflict_base.dar" "$TEST_ROOT/fixtures/append_base"
    expect_fail append_conflict_error \
        "$BIN" append -f "$TEST_ROOT/append/conflict_base.dar" "$TEST_ROOT/fixtures/append_conflict_rename"
    run_cmd append_conflict_dry_run_rename \
        "$BIN" append -f "$TEST_ROOT/append/conflict_base.dar" --dry-run --on-conflict rename \
        "$TEST_ROOT/fixtures/append_conflict_rename"
    assert_contains "$LOG_DIR/${STEP}_append_conflict_dry_run_rename.log" "conflict-1.txt"
    run_cmd append_conflict_rename \
        "$BIN" append -f "$TEST_ROOT/append/conflict_base.dar" --on-conflict rename \
        "$TEST_ROOT/fixtures/append_conflict_rename"
    run_cmd extract_append_rename \
        "$BIN" extract -f "$TEST_ROOT/append/conflict_base.dar" -d "$TEST_ROOT/append/rename_out"
    assert_file_exists "$TEST_ROOT/append/rename_out/conflict.txt"
    assert_file_exists "$TEST_ROOT/append/rename_out/conflict-1.txt"
    assert_contains "$TEST_ROOT/append/rename_out/conflict.txt" "original payload"
    assert_contains "$TEST_ROOT/append/rename_out/conflict-1.txt" "rename payload"

    run_cmd create_overwrite_base \
        "$BIN" create -f "$TEST_ROOT/append/overwrite_base.dar" "$TEST_ROOT/fixtures/append_base"
    run_cmd append_conflict_overwrite \
        "$BIN" append -f "$TEST_ROOT/append/overwrite_base.dar" --on-conflict overwrite \
        "$TEST_ROOT/fixtures/append_conflict_overwrite"
    run_cmd extract_append_overwrite \
        "$BIN" extract -f "$TEST_ROOT/append/overwrite_base.dar" -d "$TEST_ROOT/append/overwrite_out"
    assert_contains "$TEST_ROOT/append/overwrite_out/conflict.txt" "overwrite payload"

    next_step "Testing completions generation"
    mkdir -p "$TEST_ROOT/completions"
    local shell
    for shell in bash zsh fish powershell elvish; do
        run_bash "completions_${shell}" \
            "\"$BIN\" completions $shell > \"$TEST_ROOT/completions/$shell.txt\""
        assert_file_exists "$TEST_ROOT/completions/$shell.txt"
        assert_contains "$TEST_ROOT/completions/$shell.txt" "dari"
    done

    next_step "Testing encryption outputs and encrypted archive workflows"
    mkdir -p "$TEST_ROOT/encrypt"
    run_cmd create_plain_encrypt_source \
        "$BIN" create -f "$TEST_ROOT/encrypt/plain.dar" "$TEST_ROOT/fixtures/plain_small"
    run_cmd encrypt_default_output \
        "$BIN" encrypt -f "$TEST_ROOT/encrypt/plain.dar" --encrypt-passphrase secret
    assert_file_exists "$TEST_ROOT/encrypt/plain.enc.dar"
    expect_fail extract_encrypted_without_pass \
        "$BIN" extract -f "$TEST_ROOT/encrypt/plain.enc.dar" -d "$TEST_ROOT/encrypt/out_no_pass"
    expect_fail extract_encrypted_wrong_pass \
        "$BIN" extract -f "$TEST_ROOT/encrypt/plain.enc.dar" --encrypt-passphrase wrong \
        -d "$TEST_ROOT/encrypt/out_wrong_pass"
    run_cmd extract_encrypted_correct_pass \
        "$BIN" extract -f "$TEST_ROOT/encrypt/plain.enc.dar" --encrypt-passphrase secret \
        -d "$TEST_ROOT/encrypt/out_ok"
    assert_files_equal \
        "$TEST_ROOT/fixtures/plain_small/secret.txt" \
        "$TEST_ROOT/encrypt/out_ok/secret.txt"
    expect_fail append_encrypted_without_pass \
        "$BIN" append -f "$TEST_ROOT/encrypt/plain.enc.dar" "$TEST_ROOT/fixtures/append_unique_input"
    run_cmd append_encrypted_with_pass \
        "$BIN" append -f "$TEST_ROOT/encrypt/plain.enc.dar" --encrypt-passphrase secret \
        "$TEST_ROOT/fixtures/append_unique_input"
    run_cmd extract_appended_encrypted \
        "$BIN" extract -f "$TEST_ROOT/encrypt/plain.enc.dar" --encrypt-passphrase secret \
        -d "$TEST_ROOT/encrypt/out_after_append"
    assert_file_exists "$TEST_ROOT/encrypt/out_after_append/newdir/after.txt"
    run_cmd create_in_place_source \
        "$BIN" create -f "$TEST_ROOT/encrypt/in_place.dar" "$TEST_ROOT/fixtures/plain_in_place"
    run_cmd encrypt_in_place \
        "$BIN" encrypt -f "$TEST_ROOT/encrypt/in_place.dar" --encrypt-passphrase secret --in-place
    run_cmd extract_in_place_encrypted \
        "$BIN" extract -f "$TEST_ROOT/encrypt/in_place.dar" --no-index --encrypt-passphrase secret \
        -d "$TEST_ROOT/encrypt/in_place_out"
    assert_files_equal \
        "$TEST_ROOT/fixtures/plain_in_place/in_place.txt" \
        "$TEST_ROOT/encrypt/in_place_out/in_place.txt"
    inspect_and_quit "$TEST_ROOT/encrypt/plain.enc.dar" "--encrypt-passphrase secret"

    next_step "Testing chunked encryption validation and round-trip"
    mkdir -p "$TEST_ROOT/chunked"
    expect_fail create_chunked_requires_encrypt \
        "$BIN" create -f "$TEST_ROOT/chunked/invalid.dar" --chunked-encryption \
        "$TEST_ROOT/fixtures/chunked_input"
    expect_fail append_chunked_requires_v5_rejection \
        "$BIN" append -f "$TEST_ROOT/legacy_v5/basic_v5.dar" --encrypt-passphrase secret \
        --chunked-encryption "$TEST_ROOT/fixtures/append_unique_input"
    run_cmd create_chunked_archive \
        "$BIN" create -f "$TEST_ROOT/chunked/live.dar" --encrypt-passphrase secret \
        --chunked-encryption "$TEST_ROOT/fixtures/chunked_input"
    assert_file_exists "$TEST_ROOT/chunked/live.dar"
    assert_file_exists "$TEST_ROOT/chunked/live.dari"
    assert_file_exists "$TEST_ROOT/chunked/live.dar.b3"
    expect_fail extract_chunked_wrong_pass \
        "$BIN" extract -f "$TEST_ROOT/chunked/live.dar" --encrypt-passphrase wrong \
        -d "$TEST_ROOT/chunked/out_wrong"
    run_cmd extract_chunked_correct_pass \
        "$BIN" extract -f "$TEST_ROOT/chunked/live.dar" --encrypt-passphrase secret \
        -d "$TEST_ROOT/chunked/out_ok"
    assert_files_equal \
        "$TEST_ROOT/fixtures/chunked_input/chunked.bin" \
        "$TEST_ROOT/chunked/out_ok/chunked.bin"
    run_cmd append_chunked_archive \
        "$BIN" append -f "$TEST_ROOT/chunked/live.dar" --encrypt-passphrase secret \
        --chunked-encryption "$TEST_ROOT/fixtures/append_unique_input"
    run_cmd extract_chunked_after_append \
        "$BIN" extract -f "$TEST_ROOT/chunked/live.dar" --encrypt-passphrase secret \
        -d "$TEST_ROOT/chunked/out_after_append"
    assert_file_exists "$TEST_ROOT/chunked/out_after_append/newdir/after.txt"
    run_cmd verify_chunked_full \
        "$BIN" verify -f "$TEST_ROOT/chunked/live.dar" --encrypt-passphrase secret --full
    inspect_and_quit "$TEST_ROOT/chunked/live.dar" "--encrypt-passphrase secret"

    next_step "Testing incremental append workflow"
    mkdir -p "$TEST_ROOT/incremental"
    run_cmd create_incremental_base \
        "$BIN" create -f "$TEST_ROOT/incremental/live.dar" --format-version 6 \
        "$TEST_ROOT/fixtures/incremental_input"
    local since_ts
    since_ts="$(date +%s)"
    sleep 1
    cat >"$TEST_ROOT/fixtures/incremental_input/change.txt" <<'EOF'
changed payload after base archive
EOF
    cat >"$TEST_ROOT/fixtures/incremental_input/newer.txt" <<'EOF'
brand new incremental file
EOF
    run_cmd incremental_dry_run \
        "$BIN" incremental -f "$TEST_ROOT/incremental/live.dar" --dry-run \
        "$TEST_ROOT/fixtures/incremental_input"
    assert_contains "$LOG_DIR/${STEP}_incremental_dry_run.log" "change.txt"
    assert_contains "$LOG_DIR/${STEP}_incremental_dry_run.log" "newer.txt"
    assert_not_contains "$LOG_DIR/${STEP}_incremental_dry_run.log" "keep.txt"
    expect_fail incremental_append_conflict_default \
        "$BIN" incremental -f "$TEST_ROOT/incremental/live.dar" --since "@$since_ts" \
        "$TEST_ROOT/fixtures/incremental_input"
    run_cmd incremental_append \
        "$BIN" incremental -f "$TEST_ROOT/incremental/live.dar" --since "@$since_ts" \
        --on-conflict overwrite \
        "$TEST_ROOT/fixtures/incremental_input"
    run_cmd extract_incremental_archive \
        "$BIN" extract -f "$TEST_ROOT/incremental/live.dar" -d "$TEST_ROOT/incremental/out"
    assert_contains "$TEST_ROOT/incremental/out/change.txt" "changed payload after base archive"
    assert_contains "$TEST_ROOT/incremental/out/keep.txt" "original keep payload"
    assert_contains "$TEST_ROOT/incremental/out/newer.txt" "brand new incremental file"

    next_step "Testing xattr preservation and hardlink reconstruction"
    mkdir -p "$TEST_ROOT/preserve"
    expect_fail append_preserve_xattrs_requires_v6 \
        "$BIN" append -f "$TEST_ROOT/legacy_v5/basic_v5.dar" --preserve-xattrs \
        "$TEST_ROOT/fixtures/append_unique_input"
    run_cmd create_preserve_archive \
        "$BIN" create -f "$TEST_ROOT/preserve/live.dar" --preserve-xattrs \
        "$TEST_ROOT/fixtures/preserve_input"
    assert_file_exists "$TEST_ROOT/preserve/live.dar"
    assert_file_exists "$TEST_ROOT/preserve/live.dari"
    run_cmd extract_preserve_archive \
        "$BIN" extract -f "$TEST_ROOT/preserve/live.dar" -d "$TEST_ROOT/preserve/out"
    assert_files_equal \
        "$TEST_ROOT/fixtures/preserve_input/base.txt" \
        "$TEST_ROOT/preserve/out/base.txt"
    assert_files_equal \
        "$TEST_ROOT/fixtures/preserve_input/linked.txt" \
        "$TEST_ROOT/preserve/out/linked.txt"
    run_bash preserve_hardlink_check \
        "[[ \"\$(stat -c %i \"$TEST_ROOT/preserve/out/base.txt\")\" == \"\$(stat -c %i \"$TEST_ROOT/preserve/out/linked.txt\")\" ]]"
    if command -v getfattr >/dev/null 2>&1; then
        run_bash preserve_xattr_check \
            "getfattr --only-values -n user.dari.test \"$TEST_ROOT/preserve/out/base.txt\" | rg -Fq roundtrip"
    elif command -v xattr >/dev/null 2>&1; then
        run_bash preserve_xattr_check \
            "[[ \"\$(xattr -p user.dari.test \"$TEST_ROOT/preserve/out/base.txt\")\" == \"roundtrip\" ]]"
    fi

    next_step "Testing v6 archive creation, verify, reindex, no-index, and inspect"
    mkdir -p "$TEST_ROOT/v6"
    run_cmd create_v6_archive \
        "$BIN" create -f "$TEST_ROOT/v6/live.dar" --format-version 6 "$TEST_ROOT/fixtures/v6_input"
    assert_file_exists "$TEST_ROOT/v6/live.dar"
    assert_file_exists "$TEST_ROOT/v6/live.dari"
    assert_file_exists "$TEST_ROOT/v6/live.dar.b3"
    run_bash v6_list_default \
        "\"$BIN\" list -f \"$TEST_ROOT/v6/live.dar\" --json > \"$TEST_ROOT/v6/list_default.json\""
    run_bash v6_list_no_index \
        "\"$BIN\" list -f \"$TEST_ROOT/v6/live.dar\" --json --no-index > \"$TEST_ROOT/v6/list_no_index.json\""
    assert_contains "$TEST_ROOT/v6/list_default.json" "\"path\": \"nested/data.txt\""
    assert_contains "$TEST_ROOT/v6/list_no_index.json" "\"path\": \"nested/other.txt\""
    run_cmd verify_v6_full_json \
        "$BIN" verify -f "$TEST_ROOT/v6/live.dar" --full --json
    assert_contains "$LOG_DIR/${STEP}_verify_v6_full_json.log" "\"layer\": 1"
    assert_contains "$LOG_DIR/${STEP}_verify_v6_full_json.log" "\"layer\": 3"
    rm -f "$TEST_ROOT/v6/live.dari"
    assert_not_exists "$TEST_ROOT/v6/live.dari"
    run_cmd reindex_v6 \
        "$BIN" reindex -f "$TEST_ROOT/v6/live.dar"
    assert_file_exists "$TEST_ROOT/v6/live.dari"
    expect_fail reindex_v5_fails \
        "$BIN" reindex -f "$TEST_ROOT/legacy_v5/basic_v5.dar"
    run_cmd extract_v6_no_index \
        "$BIN" extract -f "$TEST_ROOT/v6/live.dar" --no-index -d "$TEST_ROOT/v6/out_no_index"
    assert_files_equal \
        "$TEST_ROOT/fixtures/v6_input/nested/data.txt" \
        "$TEST_ROOT/v6/out_no_index/nested/data.txt"
    inspect_and_quit "$TEST_ROOT/v6/live.dar" "--no-index"

    next_step "Testing split v6 archives, sidecars, list, extract, and verify"
    mkdir -p "$TEST_ROOT/split"
    run_cmd create_split_archive \
        "$BIN" create -f "$TEST_ROOT/split/archive.dar" --split-size 4K \
        "$TEST_ROOT/fixtures/split_input"
    assert_file_exists "$TEST_ROOT/split/archive.dar.001"
    assert_file_exists "$TEST_ROOT/split/archive.dar.002"
    assert_file_exists "$TEST_ROOT/split/archive.dari"
    assert_file_exists "$TEST_ROOT/split/archive.dar.001.b3"
    assert_file_exists "$TEST_ROOT/split/archive.dar.002.b3"
    run_bash split_list_json \
        "\"$BIN\" list -f \"$TEST_ROOT/split/archive.dar.001\" --json > \"$TEST_ROOT/split/list.json\""
    assert_contains "$TEST_ROOT/split/list.json" "\"path\": \"assets/random.bin\""
    run_cmd verify_split_full \
        "$BIN" verify -f "$TEST_ROOT/split/archive.dar.001" --full
    run_cmd extract_split_default \
        "$BIN" extract -f "$TEST_ROOT/split/archive.dar.001" -d "$TEST_ROOT/split/out"
    run_cmd extract_split_no_index \
        "$BIN" extract -f "$TEST_ROOT/split/archive.dar.001" --no-index -d "$TEST_ROOT/split/out_no_index"
    assert_files_equal \
        "$TEST_ROOT/fixtures/split_input/assets/random.bin" \
        "$TEST_ROOT/split/out/assets/random.bin"
    assert_files_equal \
        "$TEST_ROOT/fixtures/split_input/assets/notes.txt" \
        "$TEST_ROOT/split/out_no_index/assets/notes.txt"

    next_step "Suite completed"
    printf 'Artifacts: %s\n' "$TEST_ROOT"
    printf 'Logs: %s\n' "$LOG_DIR"
}

main "$@"
