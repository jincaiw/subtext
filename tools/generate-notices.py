#!/usr/bin/env python3
"""Write THIRD-PARTY-NOTICES.md from the resolved dependency graph.

    python3 tools/generate-notices.py
    python3 tools/generate-notices.py --check   # fail if the file is stale

The table is read from `cargo metadata`, so it covers exactly the crates a
release is built from. Re-run it whenever Cargo.lock changes.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "THIRD-PARTY-NOTICES.md"

HEADER = """\
# Third-party notices

SubText is licensed under the Apache License, Version 2.0 (see `LICENSE`).

## CotEditor

SubText is a reimplementation of [CotEditor](https://coteditor.com), and its
menus, keyboard shortcuts, syntax definitions and themes are derived from the
CotEditor project (<https://github.com/coteditor/CotEditor>). CotEditor's source
code is licensed under the Apache License, Version 2.0; its image resources are
licensed under CC BY-NC-ND 4.0. SubText bundles no CotEditor image resources.

## GPUI Kit and GPUI

The user interface is built with [GPUI Kit](https://github.com/longbridge/gpui-kit),
Copyright 2024–2026 Longbridge, licensed under the Apache License, Version 2.0.
GPUI Kit includes [GPUI](https://github.com/zed-industries/zed) from Zed
Industries, also licensed under the Apache License, Version 2.0.

## Rust dependencies

The table below is generated from `cargo metadata`; it lists every crate in the
resolved dependency graph for the supported platforms, with its declared
license. Crates offering a choice of licenses are used under the first one
listed.

"""

MIT = """\
## MIT license text

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
"""


def render() -> str:
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    workspace = set(metadata["workspace_members"])
    packages = sorted(
        (p for p in metadata["packages"] if p["id"] not in workspace),
        key=lambda p: (p["name"].lower(), p["version"]),
    )

    rows = ["| Crate | Version | License |", "| --- | --- | --- |"]
    for package in packages:
        license_name = (package.get("license") or "see repository").replace("|", "\\|")
        rows.append(f"| `{package['name']}` | {package['version']} | {license_name} |")

    return HEADER + "\n".join(rows) + "\n\n" + MIT


def main() -> None:
    check = "--check" in sys.argv[1:]
    text = render()
    if check:
        if not OUTPUT.exists() or OUTPUT.read_text() != text:
            print(
                "error: THIRD-PARTY-NOTICES.md is out of date; "
                "run python3 tools/generate-notices.py",
                file=sys.stderr,
            )
            raise SystemExit(1)
        print("THIRD-PARTY-NOTICES.md is current")
        return

    OUTPUT.write_text(text)
    crates = sum(1 for line in text.splitlines() if line.startswith("| `"))
    print(f"wrote {OUTPUT.relative_to(ROOT)} ({crates} crates)")


if __name__ == "__main__":
    main()
