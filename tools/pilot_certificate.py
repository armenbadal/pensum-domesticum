#!/usr/bin/env python3

import argparse
import base64
import json
import os
import sys
from pathlib import Path

try:
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
except ImportError:
    print(
        "Այս գործիքի համար անհրաժեշտ է Python-ի cryptography փաթեթը։\n"
        "Տեղադրիր այն՝ python3 -m pip install cryptography",
        file=sys.stderr,
    )
    raise SystemExit(1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Ստեղծում է pensum-pilot Ed25519 բանալու զույգ և դասատուի .pdcert սերտիֆիկատ։"
    )
    parser.add_argument("--secret-out", type=Path, required=True, help="Գաղտնի բանալու ուղին")
    parser.add_argument("--cert-out", type=Path, required=True, help="Սերտիֆիկատի ուղին")
    return parser.parse_args()


def repository_root() -> Path:
    script_directory = Path(__file__).resolve().parent
    for candidate in (script_directory, *script_directory.parents):
        if (candidate / ".git").exists():
            return candidate
    return Path.cwd().resolve()


def ensure_outside_repository(secret_path: Path) -> None:
    root = repository_root()
    absolute = secret_path.resolve()
    if absolute == root or root in absolute.parents:
        raise ValueError(
            "Գաղտնի բանալին մի՛ պահիր նախագծի պահոցում։ "
            "Ընտրիր պահոցից դուրս գտնվող ուղի։"
        )


def write_secret(path: Path, secret: bytes) -> None:
    if not path.parent.is_dir():
        raise ValueError(f"Գաղտնի բանալու պանակը գոյություն չունի՝ {path.parent}")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="ascii") as secret_file:
        secret_file.write(secret.hex())
        secret_file.write("\n")
        secret_file.flush()
        os.fsync(secret_file.fileno())


def rust_public_key_snippet(public_key: bytes) -> str:
    lines = []
    for offset in range(0, len(public_key), 8):
        chunk = ", ".join(f"0x{byte:02x}" for byte in public_key[offset : offset + 8])
        lines.append(f"    {chunk},")
    return (
        "pub(crate) const PILOT_PUBLIC_KEY: Option<[u8; 32]> = Some([\n"
        + "\n".join(lines)
        + "\n]);"
    )


def main() -> int:
    args = parse_args()
    secret_out = args.secret_out.expanduser()
    certificate_out = args.cert_out.expanduser()

    if certificate_out.suffix.lower() != ".pdcert":
        print("Սերտիֆիկատի ֆայլը պետք է ունենա .pdcert ընդլայնում։", file=sys.stderr)
        return 1
    if not certificate_out.parent.is_dir():
        print(f"Սերտիֆիկատի պանակը գոյություն չունի՝ {certificate_out.parent}", file=sys.stderr)
        return 1

    try:
        ensure_outside_repository(secret_out)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1

    try:
        private_key = Ed25519PrivateKey.generate()
        secret = private_key.private_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PrivateFormat.Raw,
            encryption_algorithm=serialization.NoEncryption(),
        )
        public_key = private_key.public_key().public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw,
        )

        payload = json.dumps(
            {"version": 1, "role": "teacher", "issuer_id": "pensum-pilot"},
            separators=(",", ":"),
            ensure_ascii=False,
        ).encode("utf-8")
        signature = private_key.sign(payload)
        private_key.public_key().verify(signature, payload)

        envelope = {
            "payload": base64.b64encode(payload).decode("ascii"),
            "signature": base64.b64encode(signature).decode("ascii"),
        }
        certificate = json.dumps(envelope, indent=2, ensure_ascii=False) + "\n"

        write_secret(secret_out, secret)
        certificate_out.write_text(certificate, encoding="utf-8")
    except FileExistsError:
        print(f"Գաղտնի բանալու ֆայլն արդեն գոյություն ունի՝ {secret_out}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"Ֆայլային գործողությունը ձախողվեց․ {error}", file=sys.stderr)
        return 1
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1

    print(f"Գաղտնի բանալին պահպանված է՝ {secret_out}")
    print(f"Սերտիֆիկատը պահպանված է՝ {certificate_out}")
    print(f"Հանրային բանալին (hex)՝ {public_key.hex()}")
    print()
    print("Տեղադրիր այս արժեքը src-tauri/src/certificate.rs-ում՝")
    print(rust_public_key_snippet(public_key))
    print()
    print("Գաղտնի բանալին մի՛ ավելացրու պահոց, installer կամ release artifact։")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
