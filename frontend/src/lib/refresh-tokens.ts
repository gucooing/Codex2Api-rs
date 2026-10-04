/** Token values stay in the form's memory and are never used as labels or keys. */
export function parseRefreshTokenLines(input: string) {
  const lines = input
    .split(/\r?\n/)
    .map((token, i) => ({ token: token.trim(), line: i + 1 }))
    .filter((v) => v.token);
  if (!lines.length) throw new Error("请输入 Refresh Token，一行一个");
  if (lines.length > 50) throw new Error("每批最多导入 50 条 Refresh Token");
  const seen = new Map<string, number>();
  return lines.map((item) => {
    if (item.token.length > 32768 || /[\x00-\x20\x7f]/.test(item.token))
      throw new Error(`第 ${item.line} 行不是有效的 Refresh Token`);
    const duplicateOf = seen.get(item.token);
    seen.set(item.token, duplicateOf ?? item.line);
    return { ...item, duplicateOf };
  });
}
