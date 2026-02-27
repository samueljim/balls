/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  transpilePackages: [],
  // Only use static export when building for production (pnpm build / deploy).
  // In dev (pnpm dev), omit so dynamic routes like /lobby/[id] work without
  // generateStaticParams covering every id.
  ...(process.env.NODE_ENV === "production" && { output: "export" }),
};

module.exports = nextConfig;
