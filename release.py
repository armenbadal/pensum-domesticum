#!/usr/bin/env python3
"""Prepare, validate, commit, and tag a Pensum Domesticum release."""

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent
CARGO_TOML = Path("src-tauri/Cargo.toml")
TAURI_CONFIG = Path("src-tauri/tauri.conf.json")
CARGO_LOCK = Path("src-tauri/Cargo.lock")
TEST_PLAN = Path("TEST_PLAN.md")
VERSION_PATTERN = re.compile(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\Z")

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


class ReleaseError(Exception):
    pass


def git(*args, capture=False):
    try:
        result = subprocess.run(
            ["git", *args],
            cwd=ROOT,
            check=False,
            text=True,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
        )
    except OSError as error:
        raise ReleaseError(f"Չհաջողվեց գործարկել Git-ը։ {error}") from error
    if result.returncode:
        detail = result.stderr.strip() if capture and result.stderr else ""
        raise ReleaseError(
            f"Git հրամանը ձախողվեց՝ git {' '.join(args)}"
            + (f"\n{detail}" if detail else "")
        )
    return result.stdout.strip() if capture else ""


def run_check(command, stdin=None):
    print(f"\nԳործարկվում է՝ {' '.join(command)}", flush=True)
    try:
        result = subprocess.run(command, cwd=ROOT, input=stdin, text=True, check=False)
    except OSError as error:
        raise ReleaseError(f"Չհաջողվեց գործարկել հրամանը՝ {command[0]}։ {error}") from error
    if result.returncode:
        raise ReleaseError(f"Ստուգումը ձախողվեց՝ {' '.join(command)}")


def version_tuple(version):
    if not VERSION_PATTERN.fullmatch(version):
        raise ReleaseError("Տարբերակը պետք է լինի MAJOR.MINOR.PATCH ձևաչափով, օրինակ՝ 0.3.0։")
    return tuple(int(part) for part in version.split("."))


def ensure_clean_worktree():
    status = git("status", "--porcelain=v1", "--untracked-files=all", capture=True)
    lines = status.splitlines()
    release_script_is_new = False
    unexpected = []

    for line in lines:
        if line == "?? release.py":
            release_script_is_new = True
        else:
            unexpected.append(line)

    if unexpected:
        raise ReleaseError(
            "Աշխատանքային ծառը մաքուր չէ։ Նախ commit արա կամ պահպանիր այլ փոփոխությունները։\n"
            + "\n".join(unexpected)
        )
    return release_script_is_new


def ensure_release_branch_is_synced():
    branch = git("branch", "--show-current", capture=True)
    if not branch:
        raise ReleaseError("Detached HEAD վիճակից ռելիզ անել չի թույլատրվում։")

    upstream = git(
        "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}", capture=True
    )
    if upstream != f"origin/{branch}":
        raise ReleaseError(
            f"Ընթացիկ branch-ը պետք է հետևի origin/{branch}-ին (այժմ՝ {upstream})։"
        )

    run_check(["git", "fetch", "origin"])
    ahead_behind = git("rev-list", "--left-right", "--count", "HEAD...@{upstream}", capture=True)
    if ahead_behind != "0\t0":
        raise ReleaseError(
            f"Branch-ը origin/{branch}-ի հետ համաժամեցված չէ (ahead/behind՝ {ahead_behind})։"
        )
    return branch


def ensure_tag_is_unused(tag):
    local_tag = git("tag", "--list", tag, capture=True)
    if local_tag:
        raise ReleaseError(f"{tag} tag-ը տեղայինում արդեն գոյություն ունի։")

    remote_tags = git(
        "ls-remote", "--tags", "origin", f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}",
        capture=True,
    )
    if remote_tags:
        raise ReleaseError(f"{tag} tag-ը origin-ում արդեն գոյություն ունի։")


def read_current_version():
    cargo_text = (ROOT / CARGO_TOML).read_text(encoding="utf-8")
    package_section = re.search(
        r"(?ms)^\[package\]\s*(.*?)(?=^\[|\Z)",
        cargo_text,
    )
    cargo_version_match = re.search(
        r'(?m)^version\s*=\s*"([^"]+)"\s*$',
        package_section.group(1) if package_section else "",
    )
    if not cargo_version_match:
        raise ReleaseError("Cargo.toml-ում package-ի տարբերակը չգտնվեց։")
    cargo_version = cargo_version_match.group(1)
    tauri_data = json.loads((ROOT / TAURI_CONFIG).read_text(encoding="utf-8"))
    tauri_version = tauri_data.get("version")
    if cargo_version != tauri_version:
        raise ReleaseError(
            f"Տարբերակները չեն համընկնում․ Cargo.toml={cargo_version}, "
            f"tauri.conf.json={tauri_version}։"
        )
    return cargo_version


def update_release_versions(version, current_version):
    cargo_path = ROOT / CARGO_TOML
    cargo_text = cargo_path.read_text(encoding="utf-8")
    cargo_text, count = re.subn(
        r'(?m)^version = "[^"]+"$',
        f'version = "{version}"',
        cargo_text,
        count=1,
    )
    if count != 1:
        raise ReleaseError("Cargo.toml-ում package-ի տարբերակը չգտնվեց։")

    tauri_path = ROOT / TAURI_CONFIG
    tauri_data = json.loads(tauri_path.read_text(encoding="utf-8"))
    tauri_data["version"] = version
    tauri_text = json.dumps(tauri_data, ensure_ascii=False, indent=2) + "\n"

    test_plan_path = ROOT / TEST_PLAN
    test_plan_text = test_plan_path.read_text(encoding="utf-8")
    test_plan_text, count = re.subn(
        rf"(նախատեսված է Windows-ի desktop հավելվածի `){re.escape(current_version)}"
        r"(` տարբերակի համար)",
        rf"\g<1>{version}\g<2>",
        test_plan_text,
        count=1,
    )
    if count != 1:
        raise ReleaseError("TEST_PLAN.md-ում ընթացիկ տարբերակի նկարագրությունը չգտնվեց։")

    cargo_path.write_text(cargo_text, encoding="utf-8")
    tauri_path.write_text(tauri_text, encoding="utf-8")
    test_plan_path.write_text(test_plan_text, encoding="utf-8")


def run_release_checks():
    for executable in ("git", "node", "cargo"):
        if shutil.which(executable) is None:
            raise ReleaseError(f"Պահանջվող հրամանը PATH-ում չկա՝ {executable}։")

    run_check(["node", "--check", "src/main.js"])
    grid_source = (ROOT / "src/components/math-grid/math-grid.js").read_text(
        encoding="utf-8"
    )
    run_check(["node", "--input-type=module", "--check"], stdin=grid_source)
    run_check(["cargo", "fmt", "--manifest-path", str(ROOT / CARGO_TOML), "--", "--check"])
    run_check(["cargo", "test", "--manifest-path", str(ROOT / CARGO_TOML)])
    run_check(["cargo", "build", "--manifest-path", str(ROOT / CARGO_TOML), "--release"])


def verify_lockfile_version(version):
    lock_text = (ROOT / CARGO_LOCK).read_text(encoding="utf-8")
    match = re.search(
        r'(?ms)^\[\[package\]\]\nname = "pensum-domesticum"\nversion = "([^"]+)"',
        lock_text,
    )
    if not match or match.group(1) != version:
        found = match.group(1) if match else "չգտնվեց"
        raise ReleaseError(f"Cargo.lock-ի փաթեթի տարբերակը չի թարմացվել ({found})։")


def main():
    parser = argparse.ArgumentParser(
        description="Փոխել տարբերակը, ստուգել, commit/push անել և թողարկել Git tag։"
    )
    parser.add_argument("version", help="Նոր տարբերակ՝ MAJOR.MINOR.PATCH, օրինակ՝ 0.3.0")
    args = parser.parse_args()

    requested_version = args.version
    requested_tuple = version_tuple(requested_version)
    current_version = read_current_version()
    current_tuple = version_tuple(current_version)
    if requested_tuple <= current_tuple:
        raise ReleaseError(
            f"Նոր տարբերակը պետք է մեծ լինի ընթացիկ {current_version} տարբերակից։"
        )

    tag = f"v{requested_version}"
    release_script_is_new = ensure_clean_worktree()
    branch = ensure_release_branch_is_synced()
    ensure_tag_is_unused(tag)

    print(f"Ընթացիկ տարբերակ՝ {current_version}։ Նոր տարբերակ՝ {requested_version}։")
    update_release_versions(requested_version, current_version)
    run_release_checks()
    verify_lockfile_version(requested_version)

    allowed_paths = {
        str(CARGO_TOML),
        str(TAURI_CONFIG),
        str(CARGO_LOCK),
        str(TEST_PLAN),
    }
    if release_script_is_new:
        allowed_paths.add("release.py")
    changed_paths = set(git("diff", "--name-only", capture=True).splitlines())
    untracked_paths = set(
        git("ls-files", "--others", "--exclude-standard", capture=True).splitlines()
    )
    unexpected_paths = (changed_paths | untracked_paths) - allowed_paths
    if unexpected_paths:
        raise ReleaseError(
            "Ստուգումների ընթացքում առաջացել են անսպասելի փոփոխություններ․\n"
            + "\n".join(sorted(unexpected_paths))
        )

    paths_to_commit = sorted(allowed_paths & (changed_paths | untracked_paths))
    if not paths_to_commit:
        raise ReleaseError("Ռելիզի համար փոփոխություններ չկան։")

    git("add", "--", *paths_to_commit)
    git(
        "commit",
        "-m",
        f"chore: release {requested_version}",
        "-m",
        "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>",
    )
    git("push", "origin", branch)
    git("tag", "-a", tag, "-m", f"Release {tag}")
    git("push", "origin", tag)

    print(
        f"\n{tag} ռելիզի commit-ը push արվեց {branch} branch և tag-ը՝ origin։ "
        "GitHub Actions-ը պետք է կառուցի ու հրապարակի Windows release-ը։"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ReleaseError as error:
        print(f"Ռելիզը դադարեցվեց․ {error}", file=sys.stderr)
        raise SystemExit(1)
