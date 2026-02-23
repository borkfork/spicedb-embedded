"""Hatch build hook to produce platform-specific wheel tags in CI.

When SPICEDB_WHEEL_PLATFORM is set (e.g. in GitHub Actions matrix), the wheel is
built with that platform tag so each runner produces a distinct filename. Otherwise
a pure Python (py3-none-any) wheel is built for local use.
"""

from __future__ import annotations

import os

from hatchling.builders.hooks.plugin.interface import BuildHookInterface


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version: str, build_data: dict) -> None:
        if self.target_name != "wheel":
            return
        platform = os.environ.get("SPICEDB_WHEEL_PLATFORM")
        if platform:
            build_data["tag"] = f"py3-none-{platform}"
            build_data["pure_python"] = False
