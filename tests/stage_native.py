"""Stage built PyO3 libraries and make their packages available to CI test jobs."""

from __future__ import annotations

import argparse
import importlib
import logging
import os
from pathlib import Path
import shutil
import sys
import sysconfig


ROOT = Path(__file__).resolve().parents[1]
PACKAGES = ("subcore-rs", "persian-lint-rs", "markup-rs")


def stage() -> None:
    suffix = sysconfig.get_config_var("EXT_SUFFIX")
    if not isinstance(suffix, str) or not suffix:
        raise ValueError("The current Python interpreter has no native extension suffix")
    package_roots = []
    for crate in PACKAGES:
        module = crate.replace("-", "_")
        root = ROOT / "rust" / crate
        library = (module + ".dll" if os.name == "nt" else
                   "lib" + module + (".dylib" if sys.platform == "darwin" else ".so"))
        source = root / "target" / "release" / library
        if not source.is_file():
            raise ValueError(f"Build {crate} with cargo build --locked --release before staging")
        package_root = root / "python"
        shutil.copy2(source, package_root / module / ("_native" + suffix))
        package_roots.append(str(package_root))
        logging.debug("Staged extension for %s using this interpreter's ABI suffix", module)
    sys.path[:0] = package_roots
    for crate in PACKAGES:
        module = importlib.import_module(crate.replace("-", "_"))
        if not getattr(module, "_native", None):
            raise ValueError(f"{crate} did not load its compiled implementation")
        logging.info("Loaded compiled implementation for %s", crate)
    existing = os.environ.get("PYTHONPATH")
    pythonpath = os.pathsep.join([*package_roots, *([existing] if existing else [])])
    environment = os.environ.get("GITHUB_ENV")
    if environment:
        with Path(environment).open("a", encoding="utf-8", newline="\n") as handle:
            handle.write("PYTHONPATH=" + pythonpath + "\n")
        logging.info("Native package paths configured for subsequent CI steps")
    else:
        logging.warning("GITHUB_ENV is absent; staged packages require PYTHONPATH in the caller")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log-level", choices=("INFO", "WARNING", "ERROR", "DEBUG"), default="INFO")
    args = parser.parse_args()
    logging.basicConfig(level=args.log_level, format="%(asctime)s [%(levelname)s] %(message)s")
    try:
        stage()
        return 0
    except (OSError, ValueError, ImportError) as error:
        logging.error("Native staging failed: %s", error)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
