// README 용 데모 GIF 를 헤드리스 Chrome 으로 녹화한다.
//
//   (www 에서) npm run build && npm start      # 다른 터미널, 기본 포트 3000
//   node scripts/record-demo.mjs [url]         # → docs/images/demo.gif
//
// 필요: Google Chrome, ffmpeg, bench/fixtures/*.jpg (cargo run --release -p pixelvault-core --example gen_bench_image)
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const url = process.argv[2] ?? "http://localhost:3000/";
const chromePath =
  process.env.CHROME ??
  (process.platform === "darwin" ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "google-chrome");
const files = [
  "crates/core/tests/fixtures/iphone-portrait.jpg",
  "bench/fixtures/photo-12mp.jpg",
  "bench/fixtures/photo-24mp.jpg",
  "crates/core/tests/fixtures/landscape.jpg",
  "crates/core/tests/fixtures/transparent.png",
].map((f) => join(root, f));

const port = 9334;
const profile = mkdtempSync(join(tmpdir(), "pixelvault-demo-"));
const frames = mkdtempSync(join(tmpdir(), "pixelvault-frames-"));
const chrome = spawn(
  chromePath,
  ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--force-dark-mode", "about:blank"],
  { stdio: "ignore" },
);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

try {
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    await sleep(200);
    try {
      target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === "page");
    } catch {}
  }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0;
  const pending = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id) pending.get(m.id)?.(m.result ?? m);
  };
  const send = (method, params = {}) =>
    new Promise((r) => {
      const i = ++id;
      pending.set(i, r);
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  const js = async (expression) =>
    (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result?.value;

  await send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 820, deviceScaleFactor: 1, mobile: false });
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] });
  await send("Page.enable");
  await send("DOM.enable");
  await send("Page.navigate", { url });
  await sleep(2500);

  let n = 0;
  const shoot = async (count = 1, gap = 120) => {
    for (let i = 0; i < count; i++) {
      const { data } = await send("Page.captureScreenshot", { format: "png" });
      writeFileSync(join(frames, `f${String(n++).padStart(4, "0")}.png`), Buffer.from(data, "base64"));
      await sleep(gap);
    }
  };

  await shoot(8);
  // 파일 선택 (DOM.setFileInputFiles: 헤드리스에서도 실제 파일을 input 에 넣을 수 있다)
  const { root: doc } = await send("DOM.getDocument");
  const { nodeId } = await send("DOM.querySelector", { nodeId: doc.nodeId, selector: "[data-testid=file-input]" });
  await send("DOM.setFileInputFiles", { nodeId, files });
  await shoot(8);

  await js(`document.querySelector('[data-testid=run]').click()`);
  // 변환이 끝날 때까지 녹화 (공이 계속 움직이는 게 포인트)
  for (let i = 0; i < 80; i++) {
    await shoot(1, 80);
    if (await js(`!document.querySelector('[data-testid=run]').disabled`)) break;
  }
  await shoot(10);
  // 아이폰 세로 사진 행을 눌러 방향 보정·GPS 제거·BlurHash 보여주기
  await js(`document.querySelector('[data-testid=items] tbody tr').click()`);
  await shoot(6);
  for (let y = 0; y <= 520; y += 40) {
    await js(`window.scrollTo(0, ${y})`);
    await shoot(1, 60);
  }
  await shoot(18);
  ws.close();

  mkdirSync(join(root, "docs/images"), { recursive: true });
  const out = join(root, "docs/images/demo.gif");
  // 2-pass: 프레임들로 최적 팔레트를 만든 뒤 그 팔레트로 GIF 인코딩 (색 번짐 방지)
  const r = spawnSync(
    "ffmpeg",
    [
      "-y", "-loglevel", "error", "-framerate", "8", "-i", join(frames, "f%04d.png"),
      "-vf", "scale=880:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
      out,
    ],
    { stdio: "inherit" },
  );
  if (r.status !== 0) throw new Error("ffmpeg failed");
  console.log(`✅ ${out} (${n} frames)`);
} finally {
  chrome.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
  rmSync(frames, { recursive: true, force: true });
}
