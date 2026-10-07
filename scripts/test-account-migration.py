"""Exercise the actual forward SQL migration and its automatic business boundaries."""
import pathlib
import sqlite3
import unittest
import json
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "crates/codex2api-storage/migrations"


class UnifiedAccounts(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.addCleanup(self.db.close)
        for path in sorted(MIGRATIONS.glob("*.sql")):
            if path.name.startswith("0070_"):
                break
            self.db.executescript(path.read_text(encoding="utf-8"))
        self.db.executescript("""
            INSERT INTO accounts VALUES('regular','user','Alice','user-hash',1,'2026-01-01','2026-01-01');
            INSERT INTO users VALUES('regular','Alice','alice@example.test',1234,7);
            INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_id,plan_type,created_at,subscription_started_at,subscription_expires_at)
            VALUES('managed','internal','', '', '', 'plus','plus','2026-01-01','2026-01-01','2027-01-01'),
                  ('collision','ALICE','virtual-hash','Virtual','v@example.test','pro','pro','2026-02-01','2026-02-01','2027-02-01'),
                  ('independent','unique','independent-hash','Independent','i@example.test','plus','plus','2026-03-01','2026-03-01',NULL);
            INSERT INTO user_subscriptions VALUES('regular','chatgpt','managed',4);
            INSERT INTO virtual_devices(id,virtual_account_id,refresh_hash,user_agent,created_at,last_login_at)
            VALUES('device','collision','fixture-refresh','client','2026-02-01','2026-02-01');
            INSERT INTO virtual_access_tokens(token_hash,device_id,expires_at) VALUES('fixture-access','device',2000000000);
            INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision)
            VALUES('collision','cloud_preferences','{"branch_format":"custom/{task_id}"}',8);
            INSERT INTO user_sessions VALUES('fixture-session','regular','csrf',2000000000);
            INSERT INTO meta(key,value) VALUES('oauth_jwt_private_key','fixture-key');
        """)
        self.db.executescript((MIGRATIONS / "0070_unified_users.sql").read_text(encoding="utf-8"))
        self.db.execute("PRAGMA foreign_keys=ON")

    def one(self, sql, parameters=()):
        return self.db.execute(sql, parameters).fetchone()

    def rejected(self, sql, parameters=()):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(sql, parameters)

    def test_migration_preserves_owners_and_credentials_without_merging(self):
        self.assertEqual(self.one("SELECT username,password_hash,wallet_cents,revision FROM regular_users"),
                         ("Alice", "user-hash", 1234, 7))
        owner, username, password = self.one("SELECT user_id,username,password_hash FROM virtual_platforms WHERE id='collision'")
        self.assertNotEqual(owner, "regular")
        self.assertTrue(username.startswith("ALICE~virtual-"))
        self.assertEqual(password, "virtual-hash")
        self.assertEqual(self.one("SELECT username FROM virtual_platforms WHERE id='independent'"), ("unique",))
        self.assertEqual(self.one("SELECT revision FROM platform_accounts WHERE id='managed'"), (4,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM regular_users"), (1,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM virtual_users"), (2,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM regular_platforms"), (1,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM virtual_platforms"), (2,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM wallet_entries"), (0,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM subscription_orders"), (0,))

    def test_platform_history_and_live_oauth_credentials_survive(self):
        self.assertEqual(self.one("SELECT virtual_account_id,refresh_hash FROM virtual_devices"), ("collision", "fixture-refresh"))
        self.assertEqual(self.one("SELECT token_hash,device_id FROM virtual_access_tokens"), ("fixture-access", "device"))
        self.assertEqual(self.one("SELECT revision FROM virtual_client_state WHERE virtual_account_id='collision'"), (8,))
        self.assertEqual(self.one("SELECT subscription_started_at,subscription_expires_at FROM platform_accounts WHERE id='collision'"), ("2026-02-01", "2027-02-01"))
        self.assertEqual(self.one("SELECT value FROM meta WHERE key='oauth_jwt_private_key'"), ("fixture-key",))
        self.assertEqual(self.one("SELECT user_id FROM user_sessions"), ("regular",))
        self.assertEqual(self.db.execute("PRAGMA foreign_key_check").fetchall(), [])
        self.assertEqual(self.one("SELECT count(*) FROM sqlite_master WHERE name IN ('virtual_accounts','user_subscriptions','virtual_principals')"), (0,))
        columns = {r[1] for r in self.db.execute("PRAGMA table_info(platform_accounts)")}
        self.assertFalse(columns & {"username", "password_hash", "name", "email"})

    def test_sql_hooks_reject_business_crossing_without_rust_checks(self):
        owner, = self.one("SELECT user_id FROM virtual_platforms WHERE id='collision'")
        self.rejected("INSERT INTO user_sessions VALUES('blocked',?,'csrf',2000000000)", (owner,))
        self.rejected("UPDATE user_sessions SET user_id=?", (owner,))
        self.rejected("UPDATE users SET wallet_cents=100 WHERE id=?", (owner,))
        self.rejected("UPDATE users SET kind='regular' WHERE id=?", (owner,))
        self.rejected("UPDATE platform_accounts SET user_id='regular' WHERE id='collision'")
        self.rejected("INSERT INTO platform_accounts(id,user_id,provider_id,plan_id,plan_type,created_at) VALUES('extra',?,'chatgpt','free','free','2026-01-01')", (owner,))
        self.rejected("""INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,amount_cents,balance_cents,created_at)
                         VALUES('blocked',?,'request','signature','system_adjustment',1,1,'2026-01-01')""", (owner,))
        self.rejected("""INSERT INTO subscription_orders(id,user_id,request_id,request_signature,provider_id,plan_id,plan_name,plan_revision,kind,unit_price_cents,duration_days,period_start_ms,period_end_ms,gross_cents,credit_cents,pricing_json,created_at_ms,quote_expires_at_ms)
                         VALUES('blocked',?,'request','signature','chatgpt','plus','Plus',1,'purchase',1,30,1,2,1,0,'{}',1,2)""", (owner,))

    def test_credential_hook_revokes_only_its_owner(self):
        owner, = self.one("SELECT user_id FROM virtual_platforms WHERE id='collision'")
        self.db.execute("UPDATE accounts SET password_hash='replacement' WHERE id=?", (owner,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM virtual_devices"), (0,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM virtual_access_tokens"), (0,))
        self.assertEqual(self.one("SELECT COUNT(*) FROM user_sessions"), (1,))
        self.assertEqual(self.one("SELECT password_hash FROM regular_users"), ("user-hash",))

    def test_fresh_database(self):
        with sqlite3.connect(":memory:") as fresh:
            for path in sorted(MIGRATIONS.glob("*.sql")):
                fresh.executescript(path.read_text(encoding="utf-8"))
            self.assertEqual(fresh.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_account_queries_prepare_against_the_migrated_schema(self):
        sources = ROOT / "crates/codex2api-storage/src"
        prepared = 0
        for name in ("users.rs", "virtual_users.rs", "platform_accounts.rs", "user_subscriptions.rs",
                     "user_store.rs", "oauth_authorization.rs", "oauth_device.rs", "wallet.rs",
                     "subscription_orders.rs", "user_usage.rs"):
            source = (sources / name).read_text(encoding="utf-8")
            for match in re.finditer(r'sqlx::query(?:_as|_scalar)?\s*\(\s*("(?:[^"\\]|\\.)*")', source):
                sql = json.loads(match[1].replace("\n", "\\n").replace("\r", "\\r"))
                with self.subTest(source=name, statement=sql[:100]):
                    self.db.execute("EXPLAIN " + sql, [None] * sql.count("?"))
                    prepared += 1
        self.assertGreater(prepared, 80)


if __name__ == "__main__":
    unittest.main()
