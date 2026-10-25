import { fileURLToPath } from "node:url";

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: false,
  turbopack: { root: fileURLToPath(new URL(".", import.meta.url)) },
  outputFileTracingRoot: fileURLToPath(new URL(".", import.meta.url)),
  // Next 16.3+ otherwise writes AGENTS.md / CLAUDE.md into every scaffolded
  // project on first `dev`, leaving a dirty tree before anything is edited.
  agentRules: false,
};

export default nextConfig;
