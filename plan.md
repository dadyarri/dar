# DARI cleanup roadmap

## Step 15. Add extract functionality to the TUI (inspect command)

TUI should be capable of extracting single hovered file or entire directory to chosen path by pressing `x` key. This
should open small floating window with path input field, by default filled with PWD of current process (ensure this is
cross-platform between linux/macos and windows)

## Step 18. Handle path conflicts on `append`

When `dari append` is called and one or more of the new files would produce an archive-relative
path that already exists in the archive, the current behaviour is silent duplication (two index
entries with the same path). This should be configurable via a `--on-conflict` flag:

- `--on-conflict error` (default) — abort the entire operation with a clear error listing the
  conflicting paths.
- `--on-conflict rename` — automatically suffix the conflicting path with `-1`, `-2`, … (e.g.
  `assets/logo.png` → `assets/logo-1.png`) until a free name is found.
- `--on-conflict overwrite` — keep only the new file; discard the existing entry from the index
  (the old data block is left as dead bytes, since the format has no compaction step yet).

The conflict check happens in the serial commit phase of `ArchiveBuilder`, before writing the new
entry. `--dry-run` should also report which files would conflict and what the resolved path would
be under `rename` mode.

Files: `src/cli.rs`, `src/commands/append.rs`, `src/archive_builder.rs`,
`locales/en.toml`, `locales/ru.toml`

## Step 19. Move generation of completions to separate subcommand

Introduce `dari completions <SHELL>` subcommand, that will write completion script to stdout. Remove this logic from `build.rs`

## Step 20. Encrypt command improvments

1. Change logic to save file from in-place by default to `<basename>.enc.dar`
2. Add `-o` argument to specify custom filename to save encrypted archive as
3. Add `-i` argument to save in-place

## Step 21. Github workflow improvements

1. Fix error building x86_64-apple-darwin: `The configuration 'macos-13-us-default' is not supported`
2. Deal with duplication of tags in `release` workflow. Mark all pushes to non-master branches as beta with number after dot (i. e. 5.0.0-beta.1) and master as regular releases.

### Step 21.2 — Detailed versioning plan

**Problem statement**

The current `release.yml` runs on every push to `master` or `v5`, reads the version from
`Cargo.toml`, and passes it as `tag_name` to `actions/create-release`. If the version in
`Cargo.toml` has not changed since the last push, GitHub returns a 422 error because the tag
already exists. Additionally, non-master branches always release at the same version string as
master (no `-beta.N` suffix), so there is no way to distinguish the two.

**Requirement summary**

- Git tags and `Cargo.toml` version must always match.
- Pushes to `master` → stable release (e.g. `v5.0.0`).
- Pushes to non-master branches → pre-release tagged as `v5.0.0-beta.1`, `v5.0.0-beta.2`, … with
  the counter incrementing automatically on each push.
- No duplicate-tag errors.

**Evaluated options**

| # | Approach | Pros | Cons |
|---|----------|------|------|
| A | **`cargo-release` + tag-triggered workflow** | Tags always equal `Cargo.toml`; `cargo release` bumps version, commits, tags, and pushes in one command; `--pre-release-tag beta` auto-increments beta counter | Requires `cargo-release` installed locally; workflow trigger changes from branch-push to tag-push |
| B | **`release-please` action** | Fully automated based on Conventional Commits; manages both `Cargo.toml` and tags | Requires strict commit message discipline; heavier setup; less control |
| C | **Current branch-trigger + in-workflow deduplication** | No new tooling | Fragile; duplicate-tag check still races; beta counter hard to maintain in pure shell |

**Recommended approach: Option A — `cargo-release` + tag-triggered workflow**

`cargo-release` is the de-facto Rust release tool. It:
- bumps the version in `Cargo.toml` according to semver level (`patch` / `minor` / `major`);
- creates a commit `chore: release vX.Y.Z`;
- creates a matching annotated git tag;
- pushes both the commit and the tag with a single command.

For pre-releases, `cargo release --pre-release-tag beta` creates `vX.Y.Z-beta.1` on first run and
auto-increments to `-beta.2`, `-beta.3`, … on subsequent calls (it reads the highest existing
pre-release number from the repository's tags).

**Implementation plan**

1. **Add `cargo-release` to the developer workflow** (documented in README / AGENTS.md; not
   installed in CI). No runtime CI dependency — the tool only runs locally before a release.
   ```sh
   # Stable release (on master):
   cargo release minor    # bumps 5.0.0 → 5.1.0, commits, tags v5.1.0, pushes
   cargo release patch    # bumps 5.1.0 → 5.1.1, commits, tags v5.1.1, pushes
   # Pre-release (on any branch):
   cargo release --pre-release-tag beta patch   # first:  v5.1.1-beta.1
   cargo release --pre-release-tag beta patch   # second: v5.1.1-beta.2
   ```

2. **Change `release.yml` trigger from branch-push to tag-push:**
   ```yaml
   on:
     push:
       tags:
         - 'v[0-9]*'
   ```
   This completely eliminates duplicate-tag errors: a release is created only when a new tag is
   pushed, and `cargo-release` guarantees the tag has never existed before.

3. **Detect stable vs. pre-release from the tag name** (instead of the branch name):
   ```yaml
   - name: Set prerelease flag
     run: |
       if [[ "${{ github.ref_name }}" == *"-"* ]]; then
         echo "PRERELEASE=true" >> $GITHUB_ENV
       else
         echo "PRERELEASE=false" >> $GITHUB_ENV
       fi
   ```
   Any tag containing a `-` (e.g. `v5.0.0-beta.1`) is a pre-release; plain `v5.0.0` is stable.

4. **Add a Cargo.toml version guard** in `create-release` to prevent a mismatch between the tag
   and `Cargo.toml` (defensive check in case someone pushes a tag manually):
   ```yaml
   - name: Verify tag matches Cargo.toml version
     run: |
       CARGO_VER=$(grep '^version' Cargo.toml | head -1 | awk -F'"' '{print $2}')
       TAG_VER="${GITHUB_REF_NAME#v}"
       if [ "$CARGO_VER" != "$TAG_VER" ]; then
         echo "ERROR: Cargo.toml version ($CARGO_VER) != tag ($TAG_VER)"
         exit 1
       fi
   ```

5. **CI workflow stays on branch-push** (no change needed) — it provides the fast feedback loop
   for every commit; `release.yml` handles publishing only when a proper tag is pushed.

**Files to change**

- `.github/workflows/release.yml` — trigger, prerelease detection, version guard
- `Cargo.toml` — no change needed at the time of this plan
- `README.md` / `AGENTS.md` — document the `cargo-release` release process
