"""Check business dependency direction and cross-provider source imports."""
from pathlib import Path
import re
import sys
import tomllib

root = Path(__file__).resolve().parents[1]
violations = []
allowed = {
    "common": set(),
    "storage": {"common"},
    "suppliers": {"common", "storage"},
    "upstream": {"common", "storage", "suppliers"},
    "oauth": {"common", "storage", "suppliers", "upstream"},
    "api": {"common", "storage", "upstream"},
    "admin": {"common", "storage", "suppliers", "upstream"},
    "user": {"common", "storage"},
}
for manifest in (root / "crates").glob("*/Cargo.toml"):
    package = tomllib.loads(manifest.read_text(encoding="utf-8"))
    name = package["package"]["name"]
    if name == "codex2api":
        continue  # The process composes business services.
    business = name.removeprefix("codex2api-")
    if business not in allowed:
        violations.append(f"{manifest.relative_to(root)}: unrecognized business crate {name}")
        continue
    sections = [package, *package.get("target", {}).values()]
    for section in sections:
        for kind in ("dependencies", "build-dependencies"):
            for alias, options in section.get(kind, {}).items():
                dependency = options.get("package", alias) if isinstance(options, dict) else alias
                if dependency.startswith("codex2api-") and dependency.removeprefix("codex2api-") not in allowed[business]:
                    violations.append(f"{manifest.relative_to(root)}: {name} cannot depend on {dependency}")
for source in [root / "crates", root / "frontend/src", root / "frontend-user/src"]:
    for provider in ("chatgpt", "grok"):
        other = "grok" if provider == "chatgpt" else "chatgpt"
        for directory in source.glob(f"**/providers/{provider}"):
            for file in directory.rglob("*"):
                if file.suffix not in (".rs", ".ts", ".tsx"):
                    continue
                pattern = rf"(?:providers\s*::\s*{other}\b|providers/{other}/|from\s+[\"'][^\"']*/{other}/)"
                for number, line in enumerate(file.read_text(encoding="utf-8").splitlines(), 1):
                    if re.search(pattern, line):
                        violations.append(f"{file.relative_to(root)}:{number}: cross-provider import")
if violations:
    print("\n".join(violations))
    sys.exit(1)
print("Business crate dependencies and ChatGPT/Grok provider import boundaries verified.")
