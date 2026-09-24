import type { NextConfig } from "next";
import { PHASE_DEVELOPMENT_SERVER } from "next/constants";

const shared: NextConfig = {
  basePath: "/admin",
  trailingSlash: true,
  images: { unoptimized: true },
  poweredByHeader: false,
};

function developmentBackend() {
  const value = process.env.CODEX2API_DEV_BACKEND_URL?.trim() || "http://127.0.0.1:8080";
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("CODEX2API_DEV_BACKEND_URL 必须是有效的 HTTP(S) 服务地址。");
  }
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.pathname !== "/" ||
    url.search ||
    url.hash
  ) {
    throw new Error(
      "CODEX2API_DEV_BACKEND_URL 只接受 HTTP(S) 协议、主机与端口，不包含账号、路径、查询或片段。",
    );
  }
  return url.origin;
}

function config(phase: string): NextConfig {
  if (phase !== PHASE_DEVELOPMENT_SERVER) return { ...shared, output: "export" };

  const backend = developmentBackend();
  return {
    ...shared,
    // Rust API 路径不带尾斜杠，开发代理必须保留原始请求路径。
    skipTrailingSlashRedirect: true,
    rewrites: async () => ({
      beforeFiles: [
        {
          source: "/admin/api/:path*",
          destination: `${backend}/admin/api/:path*`,
          basePath: false,
        },
        {
          source: "/api/oauth/chatgpt/:path*",
          destination: `${backend}/api/oauth/chatgpt/:path*`,
          basePath: false,
        },
      ],
    }),
    redirects: async () => [
      { source: "/", destination: "/admin/", permanent: false, basePath: false },
    ],
  };
}

export default config;
