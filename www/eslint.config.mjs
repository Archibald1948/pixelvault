import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    // 배포용 정적 산출물 (scripts/build-vercel-output.mjs 가 out/ 을 복사해 둔 것)
    ".vercel/**",
    "next-env.d.ts",
  ]),
]);

export default eslintConfig;
