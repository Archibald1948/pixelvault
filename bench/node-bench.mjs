// Node 에서 pkg/ 의 wasm 을 직접 불러 처리 시간을 잰다. (브라우저 없이 빠르게 비교할 때)
import { readFileSync, existsSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(here, "..", "pkg");
const wasm = await import(join(pkgDir, "pixelvault.js"));
// --target web 빌드는 브라우저용이라 fetch 로 .wasm 을 불러온다. Node 에선 바이트를 직접 넘긴다.
wasm.initSync({ module: readFileSync(join(pkgDir, "pixelvault_bg.wasm")) });

const inputs = process.argv.slice(2);
if (inputs.length === 0) {
  inputs.push(join(here, "fixtures/photo-12mp.jpg"), join(here, "fixtures/photo-24mp.jpg"));
}

const cases = [
  { format: "webp", maxWidth: 1920, quality: 82 },
  { format: "jpeg", maxWidth: 1920, quality: 82 },
  { format: "png", maxWidth: 1920, quality: 82 },
];
const RUNS = 3;

for (const path of inputs) {
  if (!existsSync(path)) {
    console.error(`skip (없음): ${path}`);
    continue;
  }
  const input = new Uint8Array(readFileSync(path));
  console.log(`\n${basename(path)} (${(input.length / 1e6).toFixed(1)} MB)`);
  for (const c of cases) {
    const times = [];
    let last;
    for (let i = 0; i < RUNS; i++) {
      const t0 = performance.now();
      const r = wasm.processImageRaw(input, c.maxWidth, undefined, c.format, c.quality, true, false);
      times.push(performance.now() - t0);
      last = { w: r.width, h: r.height, bytes: r.outputBytes };
      r.free();
    }
    times.sort((a, b) => a - b);
    console.log(
      `  → ${c.format.padEnd(4)} ${last.w}x${last.h}  ${(last.bytes / 1024).toFixed(0).padStart(5)} KB` +
        `  median ${times[Math.floor(RUNS / 2)].toFixed(0)} ms (min ${times[0].toFixed(0)})`,
    );
  }
}
