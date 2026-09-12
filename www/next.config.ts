import path from "node:path";
import type { NextConfig } from "next";

// PIXELVAULT_STATIC_EXPORT=1 이면 정적 사이트로 내보낸다(out/).
// 이 데모는 서버 기능이 하나도 없는(모든 페이지가 정적) 사이트라 그대로 배포할 수 있다.
// 평소 개발·벤치마크(next dev / next start)는 기본 모드를 쓰므로 분기해 둔다.
const staticExport = process.env.PIXELVAULT_STATIC_EXPORT === "1";

const nextConfig: NextConfig = {
  ...(staticExport ? { output: "export" as const } : {}),
  // www/ 바깥(../packages/pixelvault)에 있는 wasm 패키지를 import 하므로
  // 번들러가 레포 루트까지 파일을 찾아볼 수 있게 루트를 알려준다.
  turbopack: {
    root: path.join(__dirname, ".."),
  },
  outputFileTracingRoot: path.join(__dirname, ".."),
};

export default nextConfig;
