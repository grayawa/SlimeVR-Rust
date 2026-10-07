"""Read upstream oracle sources from a pinned revision outside the active project."""

import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

REFERENCE_COMMIT = "83941fd38e91cc91ca6b360deab5c2ae986dd1b6"


def reference_checkout(project_root: Path) -> tuple[Path, str]:
    override = os.environ.get("SLIMEVR_REFERENCE_ROOT")
    if override:
        root = Path(override).resolve()
        if not (root / "server/core/src/main/java").is_dir():
            raise ValueError("SLIMEVR_REFERENCE_ROOT must point to an upstream SlimeVR checkout")
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True
        ).strip()
        return root, revision

    cache = Path(tempfile.gettempdir()) / "slimevr-upstream-reference" / REFERENCE_COMMIT
    if (cache / "server/core/src/main/java").is_dir():
        return cache, REFERENCE_COMMIT
    archive = subprocess.run(
        ["git", "archive", "--format=tar", REFERENCE_COMMIT, "server"],
        cwd=project_root, capture_output=True, check=False,
    )
    if archive.returncode:
        raise RuntimeError(
            f"Upstream reference {REFERENCE_COMMIT} is missing from Git history. "
            "Use a full clone, fetch this revision from SlimeVR/SlimeVR-Server, "
            "or set SLIMEVR_REFERENCE_ROOT to a separate upstream checkout."
        )
    cache.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix="extract-", dir=cache.parent))
    try:
        with tarfile.open(fileobj=io.BytesIO(archive.stdout)) as stream:
            stream.extractall(staging, filter="data")
        try:
            staging.rename(cache)
        except OSError:
            if not (cache / "server/core/src/main/java").is_dir():
                raise
    finally:
        if staging.exists():
            shutil.rmtree(staging)
    return cache, REFERENCE_COMMIT


def source_path(path: Path, project_root: Path, reference_root: Path) -> str:
    """Keep fixture provenance paths stable for both upstream sources and adapters."""
    root = reference_root if path.is_relative_to(reference_root) else project_root
    return path.relative_to(root).as_posix()
