import path from "node:path";
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // www/ 바깥(../pkg)에 있는 wasm-pack 출력물을 import 하므로
  // 번들러가 레포 루트까지 파일을 찾아볼 수 있게 루트를 알려준다.
  turbopack: {
    root: path.join(__dirname, ".."),
  },
  outputFileTracingRoot: path.join(__dirname, ".."),
};

export default nextConfig;
