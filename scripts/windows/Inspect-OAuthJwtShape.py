"""Print JWT protocol metadata and claim types from one selected SQLite file.

Read-only; never print token strings, signatures, email addresses, subject IDs,
account IDs or signing keys. No network requests or credential refreshes.
"""
import base64
import json
from pathlib import Path
import sqlite3
import sys


def decode(part):
    return json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))


def shape(value):
    if isinstance(value, dict):
        return {key: shape(item) for key, item in value.items()}
    if isinstance(value, list):
        return [shape(value[0])] if value else []
    return {str: "string", int: "integer", float: "number", bool: "boolean", type(None): "null"}.get(type(value), "unknown")


path = Path(sys.argv[1]).resolve(strict=True)
with sqlite3.connect(path.as_uri() + "?mode=ro", uri=True) as connection:
    connection.execute("PRAGMA query_only=ON")
    report = {"database": str(path), "supplier_token_rows": connection.execute("SELECT COUNT(*) FROM supplier_tokens").fetchone()[0], "jwt_shapes": []}
    seen = set()
    for row in connection.execute("SELECT access_token,id_token FROM supplier_tokens"):
        for kind, token in zip(("access_token", "id_token"), row):
            try:
                parts = token.split(".")
                if len(parts) != 3:
                    continue
                header, payload = decode(parts[0]), decode(parts[1])
                metadata = {
                    "kind": kind,
                    "header": {key: (value if key in ("alg", "typ") else shape(value)) for key, value in header.items()},
                    "protocol": {key: payload[key] for key in ("iss", "aud", "scope", "scp", "azp", "client_id") if key in payload},
                    "claims": shape(payload),
                }
                if isinstance(payload.get("iat"), int) and isinstance(payload.get("exp"), int):
                    metadata["lifetime_seconds"] = payload["exp"] - payload["iat"]
                serialized = json.dumps(metadata, sort_keys=True)
                if serialized not in seen:
                    report["jwt_shapes"].append(metadata)
                    seen.add(serialized)
            except (TypeError, ValueError, AttributeError, IndexError):
                continue
    print(json.dumps(report, ensure_ascii=True, indent=2))
