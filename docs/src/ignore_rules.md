# Ignore Rules

**dari** uses the [`ignore`](https://docs.rs/ignore) crate for directory traversal. It
respects the standard ignore rules used in the Git ecosystem.

## Supported Rule Sources

### .gitignore

**dari** automatically reads `.gitignore` files found during directory traversal. Files
and directories listed in `.gitignore` are excluded from the archive. This ensures that
build artifacts, `node_modules` directories, caches, and other ignored files never end up
in the archive.

### .darignore

In addition to `.gitignore`, **dari** reads `.darignore` — a project-specific exclusion
list for the archiver. Its syntax is identical to `.gitignore`.

Example `.darignore`:

```gitignore
# Exclude the archive file itself
*.dar

# Exclude temporary directories
tmp/
scratch/
```

## Hidden Files

By default **dari** **includes** hidden files (those starting with `.`) in the archive.
This differs from the default behaviour of `find` and many other tools.

To exclude specific hidden files or directories (e.g. `.git/`), add them to `.gitignore`
or `.darignore`.

## Individual File Arguments

If a path to an individual file (rather than a directory) is passed as a command
argument, it is added to the archive **regardless of ignore rules**. The `.gitignore` and
`.darignore` rules are applied only during recursive directory traversal.

## Rule Application Order

During directory traversal, rules are applied in the following order:

1. Rules from `.gitignore` (all files found along the path).
2. Rules from `.darignore` (all files found along the path).

A file is excluded if it matches at least one rule from any source.

