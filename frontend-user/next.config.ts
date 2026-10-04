import type { NextConfig } from "next";
import { PHASE_DEVELOPMENT_SERVER } from "next/constants";

export default function config(phase: string): NextConfig {
  const shared: NextConfig = {
    basePath: "/user",
    trailingSlash: true,
    images: { unoptimized: true },
    poweredByHeader: false,
  };
  if (phase !== PHASE_DEVELOPMENT_SERVER) return { ...shared, output: "export" };
  const url = new URL(process.env.CODEX2API_DEV_USER_BACKEND_URL || "http://127.0.0.1:8082");
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.pathname !== "/" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  )
    throw new Error("用户后端地址必须为 HTTP(S) origin");
  return {
    ...shared,
    skipTrailingSlashRedirect: true,
    rewrites: async () => ({
      beforeFiles: [
        {
          source: "/user/api/:path*",
          destination: `${url.origin}/user/api/:path*`,
          basePath: false,
        },
      ],
    }),
    redirects: async () => [
      { source: "/", destination: "/user/", permanent: false, basePath: false },
    ],
  };
}
