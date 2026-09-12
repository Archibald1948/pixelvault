// wasm-pack 출력(/pkg)을 패키지 안(src/wasm 또는 dist/wasm)으로 복사한다.
//   node scripts/copy-wasm.mjs src   — 타입체크용 (.d.ts 가 있어야 import 가 타입을 가진다)
//   node scripts/copy-wasm.mjs dist  — 배포용 (.js glue + .wasm)
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "../../../pkg");
const target = join(here, "..", process.argv[2] ?? "dist", "wasm");

const files = ["pixelvault.js", "pixelvault.d.ts", "pixelvault_bg.wasm", "pixelvault_bg.wasm.d.ts"];
if (!existsSync(join(pkg, files[0]))) {
  console.error("pkg/ 가 없습니다. 먼저 레포 루트에서 ./scripts/build-wasm.sh 를 실행하세요.");
  process.exit(1);
}
mkdirSync(target, { recursive: true });
for (const f of files) copyFileSync(join(pkg, f), join(target, f));
console.log(`copied wasm → ${target}`);
