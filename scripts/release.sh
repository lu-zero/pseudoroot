#!/usr/bin/env bash
# Cuts a new pseudoroot release: bumps the workspace version everywhere it's
# duplicated (workspace.package, the internal pseudoroot/pseudoroot-core path
# dep constraints, and the throwaway embed-manifest build.rs synthesizes for
# the interposed cdylib), verifies the workspace still builds/lints/tests
# clean, then commits and tags.
#
# By default this only touches the local working tree + git history — it
# does not push or publish. Pass --push to push the branch and tag, and
# --publish (implies --push) to also run `cargo publish` in dependency
# order (pseudoroot-core, then pseudoroot) once the former is live on the
# index.
#
#   scripts/release.sh <new-version> [-m <notes>] [--push] [--publish] [-y]
#
#   scripts/release.sh 0.2.3
#   scripts/release.sh 0.2.3 -m "Ship the daemon reconnect fix." --push
#   scripts/release.sh 0.2.3 --publish -y
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

usage() {
    sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
}

new_version=""
notes=""
do_push=0
do_publish=0
assume_yes=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        -m|--notes) notes="$2"; shift 2 ;;
        --push) do_push=1; shift ;;
        --publish) do_publish=1; do_push=1; shift ;;
        -y|--yes) assume_yes=1; shift ;;
        -h|--help) usage ;;
        -*) echo "unknown flag: $1" >&2; usage ;;
        *)
            if [[ -n "$new_version" ]]; then
                echo "unexpected extra argument: $1" >&2
                usage
            fi
            new_version="$1"
            shift
            ;;
    esac
done

[[ -n "$new_version" ]] || { echo "usage: scripts/release.sh <new-version> [-m <notes>] [--push] [--publish] [-y]" >&2; exit 1; }
[[ "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || {
    echo "error: '$new_version' doesn't look like a semver version (X.Y.Z)" >&2
    exit 1
}

confirm() { # <prompt>
    [[ "$assume_yes" -eq 1 ]] && return 0
    read -r -p "$1 [y/N] " reply
    [[ "$reply" =~ ^[Yy]$ ]]
}

if [[ -n "$(git status --porcelain)" ]]; then
    echo "error: working tree is dirty; commit or stash first" >&2
    exit 1
fi

old_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[[ -n "$old_version" ]] || { echo "error: couldn't find [workspace.package] version in Cargo.toml" >&2; exit 1; }
if [[ "$old_version" == "$new_version" ]]; then
    echo "error: $new_version is already the current version" >&2
    exit 1
fi
if [[ "$(printf '%s\n%s\n' "$old_version" "$new_version" | sort -V | tail -1)" != "$new_version" ]]; then
    confirm "warning: $new_version is not greater than current $old_version — continue anyway?" || exit 1
fi

echo "# bumping $old_version -> $new_version"

branch="$(git branch --show-current)"

revert_bump() { # <reason>
    echo "$1 — reverting version bump" >&2
    git checkout -- Cargo.toml pseudoroot/build.rs
}

bump() { # <file> <sed-expr> <description>
    local before after
    before="$(git diff --stat -- "$1")"
    sed -i "$2" "$1"
    after="$(git diff --stat -- "$1")"
    if [[ "$before" == "$after" ]]; then
        revert_bump "error: expected to bump $3 in $1 but nothing changed (pattern out of date?)"
        exit 1
    fi
}

bump Cargo.toml "s/^version = \"$old_version\"\$/version = \"$new_version\"/" "workspace.package version"
bump Cargo.toml "s/pseudoroot = { version = \"$old_version\", path = \"pseudoroot\" }/pseudoroot = { version = \"$new_version\", path = \"pseudoroot\" }/" "pseudoroot path-dep version"
bump Cargo.toml "s/pseudoroot-core = { version = \"$old_version\", path = \"pseudoroot-core\" }/pseudoroot-core = { version = \"$new_version\", path = \"pseudoroot-core\" }/" "pseudoroot-core path-dep version"
bump pseudoroot/build.rs "s/^version = \"$old_version\"\$/version = \"$new_version\"/" "embed-manifest version"
bump pseudoroot/build.rs "s/pseudoroot-core = \"$old_version\"/pseudoroot-core = \"$new_version\"/" "embed-manifest pseudoroot-core constraint"

leftover="$(grep -rn "\"$old_version\"" --include='*.toml' --include='*.rs' . 2>/dev/null | grep -v '/target/' || true)"
if [[ -n "$leftover" ]]; then
    echo "warning: '$old_version' still appears after the bump — check these by hand:" >&2
    echo "$leftover" >&2
fi

echo "# verifying: build, clippy, test"
if ! { cargo build --workspace --all-targets \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace; }; then
    revert_bump "error: verification failed"
    exit 1
fi

git diff -- Cargo.toml pseudoroot/build.rs
if ! confirm "commit this as 'chore: bump to $new_version'?"; then
    revert_bump "aborted"
    exit 1
fi

git add Cargo.toml pseudoroot/build.rs
commit_body="chore: bump to $new_version"
if [[ -n "$notes" ]]; then
    commit_body="$commit_body

$notes"
fi
git commit -m "$commit_body"
git tag -a "v$new_version" -m "v$new_version"
echo "# committed and tagged v$new_version"

if [[ "$do_push" -eq 0 ]]; then
    echo "# next: git push origin $branch v$new_version"
    exit 0
fi

confirm "push $branch and tag v$new_version to origin?" || exit 0
git push origin "$branch" "v$new_version"

# pseudoroot-daemon and pseudoroot both depend on pseudoroot-core but not on
# each other, so pseudoroot-core must land first; the other two can publish
# in either order once it's visible.
if [[ "$do_publish" -eq 0 ]]; then
    echo "# next: cargo publish -p pseudoroot-core, wait for it on crates.io, then cargo publish -p pseudoroot-daemon and cargo publish -p pseudoroot"
    exit 0
fi

confirm "publish pseudoroot-core $new_version to crates.io?" || exit 0
cargo publish -p pseudoroot-core

echo "# waiting for pseudoroot-core $new_version to show up on crates.io..."
for _ in $(seq 1 30); do
    if cargo info "pseudoroot-core@$new_version" >/dev/null 2>&1; then
        break
    fi
    sleep 5
done
if ! cargo info "pseudoroot-core@$new_version" >/dev/null 2>&1; then
    echo "error: pseudoroot-core $new_version still not visible on crates.io after 2.5 minutes" >&2
    echo "       retry manually once it lands: cargo publish -p pseudoroot-daemon && cargo publish -p pseudoroot" >&2
    exit 1
fi

published=(pseudoroot-core)
for pkg in pseudoroot-daemon pseudoroot; do
    if confirm "publish $pkg $new_version to crates.io?"; then
        cargo publish -p "$pkg"
        published+=("$pkg")
    fi
done
echo "# published: ${published[*]} @ $new_version"
