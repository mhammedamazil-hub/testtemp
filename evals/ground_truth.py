"""Ground truth fetched independently of the browser.

Values come from public APIs (GitHub REST, PyPI JSON) that never touch Obscura,
so an agent answer can be checked against a source it cannot influence.
Recorded fallbacks are used if an API is unreachable; ``live`` records which.
"""

from __future__ import annotations

import json
import subprocess
import urllib.request
from dataclasses import dataclass, field
from typing import Dict, Optional


@dataclass
class GroundTruth:
    values: Dict[str, Optional[str]] = field(default_factory=dict)
    live: Dict[str, bool] = field(default_factory=dict)

    def get(self, key: str) -> Optional[str]:
        return self.values.get(key)

    def is_live(self, key: str) -> bool:
        return self.live.get(key, False)

    def to_dict(self) -> Dict[str, object]:
        return {"values": self.values, "live": self.live}


def _http_json(url: str, timeout: int = 25) -> Optional[dict]:
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "obscura-agent-eval"})
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except Exception:
        return None


def _gh_api(path: str) -> Optional[dict]:
    try:
        out = subprocess.run(["gh", "api", path], capture_output=True, text=True, timeout=30)
        if out.returncode != 0:
            return None
        return json.loads(out.stdout)
    except Exception:
        return None


#: Fallbacks recorded on 2026-10-25. Used only if the API is unreachable.
_RECORDED = {
    "obscura_latest_release": "v0.2.5",
    "obscura_license": "Apache-2.0",
    "requests_version": "2.34.2",
    "requests_requires_python": ">=3.10",
}


def collect() -> GroundTruth:
    truth = GroundTruth()

    release = _gh_api("repos/h4ckf0r0day/obscura/releases/latest")
    if release and release.get("tag_name"):
        truth.values["obscura_latest_release"] = release["tag_name"]
        truth.live["obscura_latest_release"] = True
    else:
        truth.values["obscura_latest_release"] = _RECORDED["obscura_latest_release"]
        truth.live["obscura_latest_release"] = False

    repo = _gh_api("repos/h4ckf0r0day/obscura")
    license_id = ((repo or {}).get("license") or {}).get("spdx_id")
    if license_id:
        truth.values["obscura_license"] = license_id
        truth.live["obscura_license"] = True
    else:
        truth.values["obscura_license"] = _RECORDED["obscura_license"]
        truth.live["obscura_license"] = False

    pypi = _http_json("https://pypi.org/pypi/requests/json")
    info = (pypi or {}).get("info") or {}
    if info.get("version"):
        truth.values["requests_version"] = info["version"]
        truth.values["requests_requires_python"] = info.get("requires_python")
        truth.live["requests_version"] = True
        truth.live["requests_requires_python"] = True
    else:
        truth.values["requests_version"] = _RECORDED["requests_version"]
        truth.values["requests_requires_python"] = _RECORDED["requests_requires_python"]
        truth.live["requests_version"] = False
        truth.live["requests_requires_python"] = False

    return truth
