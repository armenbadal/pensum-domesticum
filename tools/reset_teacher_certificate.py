#!/usr/bin/env python3

import argparse
import json
import os
import sys
from pathlib import Path

DEFAULT_IDENTIFIER = "armen.badalian.pensum-domesticum"
CERTIFICATE_NAME = "teacher.pdcert"


def read_identifier() -> str:
    config_path = (
        Path(__file__).resolve().parent.parent / "src-tauri" / "tauri.conf.json"
    )
    try:
        with config_path.open(encoding="utf-8") as config_file:
            identifier = json.load(config_file).get("identifier")
        if isinstance(identifier, str) and identifier.strip():
            return identifier
    except (OSError, ValueError):
        pass
    return DEFAULT_IDENTIFIER


def default_certificate_path() -> Path:
    if sys.platform == "win32":
        base = os.environ.get("APPDATA") or Path.home() / "AppData" / "Roaming"
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config"
    return Path(base) / read_identifier() / CERTIFICATE_NAME


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Ջնջում է հավելվածում ներմուծված դասատուի հավաստագիրը։"
    )
    parser.add_argument("--file", type=Path, help="Հավաստագրի ճշգրիտ ուղին")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Միայն ցույց տալ ուղին՝ առանց ֆայլը ջնջելու",
    )
    args = parser.parse_args()

    certificate_path = args.file.expanduser() if args.file else default_certificate_path()
    print(f"Հավաստագրի ուղին՝ {certificate_path}")

    if args.dry_run:
        print("Չոր փորձ․ ֆայլը չի ջնջվել։")
        return 0

    try:
        certificate_path.unlink()
    except FileNotFoundError:
        print("Հավաստագիր չի գտնվել․ հավելվածն արդեն սովորողի ռեժիմում է։")
        return 0
    except OSError as error:
        print(f"Չհաջողվեց ջնջել հավաստագիրը․ {error}", file=sys.stderr)
        return 1

    print("Հավաստագիրը ջնջվեց։ Վերագործարկիր հավելվածը, որ այն անցնի սովորողի ռեժիմի։")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
