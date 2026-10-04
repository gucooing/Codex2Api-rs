"""Reject imports or path inclusions from one channel into the other."""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
violations = []
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
print("ChatGPT and Grok provider import boundaries verified.")
