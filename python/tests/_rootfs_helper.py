# SPDX-License-Identifier: Apache-2.0
"""Locate the static rootfs-helper test fixture."""

from __future__ import annotations

import functools
import subprocess
from pathlib import Path

_REPO = Path(__file__).resolve().parents[2]


@functools.cache
def rootfs_helper() -> Path:
    """The rootfs-helper that the sandlock-fixtures crate builds."""
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "sandlock-fixtures"],
        cwd=_REPO, check=True, capture_output=True, text=True,
    )
    return Path(out.stdout.strip())
