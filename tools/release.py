"""Release chores for CashGear Engine: bump the version, check a tag, print release notes.

    python tools/release.py bump 0.2.0     # the workspace version, the template's engine tag,
                                           # the guide's --tag, and CHANGELOG's [Unreleased]
    python tools/release.py check v0.2.0   # everything agrees with the tag (CI runs this)
    python tools/release.py notes 0.2.0    # CHANGELOG's section for that version

Releasing: bump, write the CHANGELOG section, commit, then tag `vX.Y.Z` and push the tag.
The release workflow (.github/workflows/release.yml) builds the platformer for Windows, the
web and Android and publishes them in a GitHub Release with these notes.
"""
import datetime
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / "Cargo.toml"
TEMPLATE = ROOT / "templates" / "game" / "Cargo.toml"
CHANGELOG = ROOT / "CHANGELOG.md"
# Files that name the release tag users install from.
TAGGED = [
    TEMPLATE,
    ROOT / "templates" / "game" / "README.md",
    ROOT / "templates" / "game" / "cargo-generate.toml",
    ROOT / "docs" / "book" / "src" / "getting-started.md",
    ROOT / "README.md",
]
VERSION = re.compile(r"^\d+\.\d+\.\d+$")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="")


def workspace_version() -> str:
    match = re.search(r'\[workspace\.package\]\s*\nversion = "([^"]+)"', read(WORKSPACE))
    if not match:
        sys.exit("no [workspace.package] version in Cargo.toml")
    return match.group(1)


def section(version: str) -> str | None:
    text = read(CHANGELOG)
    match = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    return match.group(1).strip() if match else None


def bump(version: str) -> None:
    if not VERSION.match(version):
        sys.exit(f"not a version: {version}")
    old = workspace_version()
    text = read(WORKSPACE)
    write(WORKSPACE, text.replace(f'[workspace.package]\nversion = "{old}"',
                                  f'[workspace.package]\nversion = "{version}"', 1))
    for path in TAGGED:
        if path.exists():
            write(path, read(path).replace(f"v{old}", f"v{version}"))
    log = read(CHANGELOG)
    today = datetime.date.today().isoformat()
    if "## [Unreleased]" in log:
        write(CHANGELOG, log.replace("## [Unreleased]", f"## [{version}] — {today}", 1))
    print(f"{old} -> {version}; now write CHANGELOG.md's [{version}] section and run cargo build")


def check(tag: str) -> None:
    version = tag.removeprefix("v")
    problems = []
    if workspace_version() != version:
        problems.append(f"Cargo.toml's version is {workspace_version()}, the tag {tag}")
    for path in TAGGED:
        tags = set(re.findall(r"\bv\d+\.\d+\.\d+\b", read(path)))
        if tags and tags != {tag}:
            problems.append(f"{path.relative_to(ROOT)} names {', '.join(sorted(tags))}")
    if section(version) is None:
        problems.append(f"CHANGELOG.md has no [{version}] section")
    if problems:
        sys.exit("\n".join(problems))
    print(f"{tag}: the version, the template, the guide and the changelog agree")


def notes(version: str) -> None:
    body = section(version.removeprefix("v"))
    if body is None:
        sys.exit(f"CHANGELOG.md has no [{version}] section")
    print(body)


if __name__ == "__main__":
    commands = {"bump": bump, "check": check, "notes": notes}
    if len(sys.argv) != 3 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2])
