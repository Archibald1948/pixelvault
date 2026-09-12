// 벤치마크 페이지(/bench)를 헤드리스 Chrome 에서 돌리고 결과 마크다운을 저장한다.
// 가려진 탭처럼 CPU 우선순위가 깎이지 않아서 README 용 숫자를 뽑기에 좋다.
//
//   (www 에서) npm run build && npm start      # 다른 터미널
//   node bench/browser-bench.mjs [url]         # 기본 http://localhost:3000/bench
//
// 환경변수: CHROME=/path/to/chrome, RUNS=5, FORMATS=webp,jpeg,png
// 외부 의존성 없이 Node 22 의 내장 WebSocket 으로 Chrome DevTools Protocol 을 직접 쓴다.
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const url = process.argv[2] ?? "http://localhost:3000/bench";
const chromePath =
  process.env.CHROME ??
  (process.platform === "darwin"
    ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    : "google-chrome");
const runs = Number(process.env.RUNS ?? 5);
const formats = (process.env.FORMATS ?? "webp,jpeg").split(",");
const port = 9333;

const profile = mkdtempSync(join(tmpdir(), "pixelvault-bench-"));
const chrome = spawn(
  chromePath,
  [
    "--headless=new",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ],
  { stdio: "ignore" },
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

try {
  // DevTools 엔드포인트가 뜰 때까지 대기
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    await sleep(200);
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      target = list.find((t) => t.type === "page");
    } catch {}
  }
  if (!target) throw new Error("Chrome DevTools 에 연결하지 못했습니다");

  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r, j) => ((ws.onopen = r), (ws.onerror = j)));
  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const mid = ++id;
      pending.set(mid, resolve);
      ws.send(JSON.stringify({ id: mid, method, params }));
    });
  const evaluate = async (expression) => {
    const res = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (res.result?.exceptionDetails) throw new Error(JSON.stringify(res.result.exceptionDetails));
    return res.result?.result?.value;
  };

  await send("Page.enable");
  await send("Page.navigate", { url });
  for (let i = 0; i < 100; i++) {
    await sleep(200);
    if ((await evaluate("typeof window.__pixelvaultBench")) === "function") break;
  }

  console.log(`▶ ${url} — runs=${runs}, formats=${formats.join(",")} (수십 초 걸립니다)`);
  const result = await evaluate(
    `window.__pixelvaultBench(${JSON.stringify({ runs, formats })})`,
  );
  console.log("\n" + result.markdown + "\n");

  const here = dirname(fileURLToPath(import.meta.url));
  mkdirSync(join(here, "results"), { recursive: true });
  const out = join(here, "results", `${new Date().toISOString().slice(0, 10)}-headless-chrome.md`);
  writeFileSync(out, result.markdown + "\n\n```json\n" + JSON.stringify(result.rows, null, 2) + "\n```\n");
  console.log(`저장: ${out}`);
  ws.close();
} finally {
  chrome.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
