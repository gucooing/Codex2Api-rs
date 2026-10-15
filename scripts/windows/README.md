# Desktop 合约测试

产品入口已改为 [C# 图形启动器](../../tools/desktop-proxy/README.md)，不再提供独立 profile、复制程序或 app-server shim 的启动脚本。

此目录只保留供开发验证使用的 Desktop 接口与地址 hook 测试。测试读取实际安装包；安装包路径由参数或测试环境变量传入。测试中的临时数据目录只隔离测试账号，不属于产品启动行为。

`Inspect-OAuthJwtShape.py` 从 `CODEX2API_DATABASE_URL` 指定的 PostgreSQL 只读检查 JWT 结构，需要 `psycopg`（可用 `python -m pip install "psycopg[binary]"` 安装）。它只输出协议字段和类型，不输出令牌或身份值。
