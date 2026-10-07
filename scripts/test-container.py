"""Smoke-test a local image using Compose and a disposable host data directory."""

import html
import http.cookiejar
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import uuid


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python3 scripts/test-container.py LOCAL_IMAGE")

    root = Path(__file__).resolve().parent.parent
    env = os.environ.copy()
    env["CODEX2API_IMAGE"] = sys.argv[1]
    for surface, port in (("API", 8080), ("ADMIN", 8081), ("USER", 8082)):
        env[f"CODEX2API_{surface}_HOST"] = "127.0.0.1"
        env[f"CODEX2API_{surface}_PORT"] = "0"
        env[f"CODEX2API_PUBLIC_{surface}_URL"] = f"http://127.0.0.1:{port}"

    # Never read the operator's .env or touch an existing deployment's data.
    with tempfile.TemporaryDirectory(prefix="codex2api-container-") as temporary:
        env_file = Path(temporary) / "empty.env"
        env_file.touch()
        data_dir = Path(temporary) / "data"
        data_dir.mkdir()
        env["CODEX2API_DATA_DIR"] = str(data_dir)
        compose = [
            "docker", "compose", "--project-name", f"codex2api-smoke-{uuid.uuid4().hex[:12]}",
            "--env-file", str(env_file), "--file", str(root / "docker-compose.yml"),
        ]

        def run(*args, capture=False, check=True):
            return subprocess.run(
                [*compose, *args], cwd=root, env=env, text=True,
                stdout=subprocess.PIPE if capture else None, check=check,
            )

        def origin(port):
            return "http://" + run("port", "codex2api", str(port), capture=True).stdout.strip()

        def data_owner(owner):
            # Only this test's freshly created directory is mounted or chowned.
            subprocess.run([
                "docker", "run", "--rm", "--network", "none", "--user", "0:0",
                "--entrypoint", "chown", "--mount",
                f"type=bind,source={data_dir},target=/app/data", sys.argv[1],
                "-R", owner, "/app/data",
            ], check=True)

        cookies = http.cookiejar.CookieJar()
        client = urllib.request.build_opener(
            urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(cookies)
        )

        def request(url, *, statuses=(200,), data=None):
            headers = {"Content-Type": "application/json"} if data is not None else {}
            req = urllib.request.Request(
                url, headers=headers,
                data=json.dumps(data).encode() if data is not None else None,
            )
            try:
                response = client.open(req, timeout=15)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                if response.status not in statuses:
                    raise AssertionError(f"{url}: expected {statuses}, got {response.status}")
                return response.read()

        def check_surfaces():
            api, admin, user = (origin(port) for port in (8080, 8081, 8082))
            assert json.loads(request(api + "/healthz"))["ok"] is True
            for base, path in ((admin, "/admin/"), (user, "/user/")):
                page = request(base + path).decode()
                assert "<html" in page.lower(), f"Missing embedded page at {base}{path}"
                asset = re.search(r'(?:src|href)="([^\"]*/_next/static/[^\"]+)"', page)
                assert asset, f"Missing Next.js assets at {base}{path}"
                assert request(urllib.parse.urljoin(base, html.unescape(asset[1])))
            for base, paths in (
                (api, ("/admin/", "/admin/api/session", "/user/")),
                (user, ("/admin/", "/admin/api/session")),
                (admin, ("/user/", "/user/api/session")),
            ):
                for path in paths:
                    request(base + path, statuses=(401, 404))
            return admin

        try:
            data_owner("10001:10001")
            run("config", "--quiet")
            run("up", "--detach", "--wait", "--wait-timeout", "180", "--no-build", "--pull", "never")
            assert run("exec", "-T", "codex2api", "id", "-u", capture=True).stdout.strip() == "10001"
            admin = check_surfaces()
            request(admin + "/admin/api/session", statuses=(401,))
            request(admin + "/admin/api/login", data={"username": "admin", "password": "admin"})
            request(admin + "/admin/api/session")
            run("exec", "-T", "codex2api", "test", "-s", "/app/data/codex2api.sqlite")

            # Recreate the container, retaining only its host folder. The same session
            # must still work, including the persisted signing key and identity.
            run("up", "--detach", "--force-recreate", "--wait", "--wait-timeout", "180",
                "--no-build", "--pull", "never")
            admin = check_surfaces()
            request(admin + "/admin/api/session")
            assert (data_dir / "codex2api.sqlite").stat().st_size > 0
            print("Container smoke test passed: three listeners, embedded assets, non-root execution and bind-mounted SQLite persistence.")
        except Exception:
            run("logs", "--no-color", check=False)
            raise
        finally:
            # The generated project name limits cleanup to this test's resources.
            run("down", "--remove-orphans", check=False)
            if hasattr(os, "getuid"):
                data_owner(f"{os.getuid()}:{os.getgid()}")


if __name__ == "__main__":
    main()
