// Canvas API(순수 JS) vs WASM 벤치마크 러너.
import type { OutputFormat, PixelVault } from "@sc0031/pixelvault";
import { decodeToImageData, psnr, ssim } from "./metrics";

export type Pipeline = "canvas" | "wasm";

export interface BenchConfig {
  maxWidth: number;
  quality: number;
  formats: OutputFormat[];
  /** 측정 반복 횟수 (워밍업 1회는 별도) */
  runs: number;
}

export interface BenchRow {
  image: string;
  imageBytes: number;
  sourceSize: string;
  format: OutputFormat;
  pipeline: Pipeline;
  /** 요청한 포맷과 실제 결과 MIME 이 다를 수 있다(예: Safari 캔버스는 WebP 인코딩 미지원 → PNG) */
  actualMime: string;
  medianMs: number;
  minMs: number;
  outputBytes: number;
  width: number;
  height: number;
  /** 각 방식의 "무손실 리사이즈 결과" 대비 SSIM — 인코딩으로 잃은 품질 */
  ssim: number;
  psnr: number;
  /** 처리 1회 동안 메인 스레드가 가장 오래 멈춘 시간 */
  mainThreadMaxGapMs: number;
  blob: Blob;
}

const MIME: Record<OutputFormat, string> = {
  webp: "image/webp",
  jpeg: "image/jpeg",
  png: "image/png",
};

export function fitWithin(w: number, h: number, maxWidth: number): [number, number] {
  const scale = Math.min(1, maxWidth / w);
  return [Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale))];
}

// ── Canvas 파이프라인: 대부분의 튜토리얼/라이브러리가 쓰는 방식 ─────────────────
async function canvasOnce(file: Blob, format: OutputFormat, cfg: BenchConfig) {
  // 1) 디코드 (EXIF 방향은 브라우저가 반영)
  const bmp = await createImageBitmap(file, { imageOrientation: "from-image" });
  const [w, h] = fitWithin(bmp.width, bmp.height, cfg.maxWidth);
  // 2) 리사이즈: drawImage + 고품질 스무딩
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bmp, 0, 0, w, h);
  bmp.close();
  // 3) 인코드
  const blob = await canvas.convertToBlob({ type: MIME[format], quality: cfg.quality / 100 });
  return { blob, w, h, ctx };
}

async function wasmOnce(vault: PixelVault, file: Blob, format: OutputFormat, cfg: BenchConfig) {
  const r = await vault.process(file, { format, maxWidth: cfg.maxWidth, quality: cfg.quality });
  return { blob: new Blob([r.bytes as BlobPart], { type: r.mimeType }), w: r.width, h: r.height };
}

/**
 * 메인 스레드 멈춤 측정기. MessageChannel 로 메시지를 계속 주고받으며 이벤트 사이 최대 간격을 잰다.
 * (setTimeout/rAF 와 달리 백그라운드 탭에서도 스로틀링되지 않는다)
 * 측정 중에는 메인 스레드 코어 하나를 계속 쓰므로, 시간 측정과는 따로 돌린다.
 */
async function measureMainThreadGap(work: () => Promise<unknown>): Promise<number> {
  const ch = new MessageChannel();
  let last = performance.now();
  let maxGap = 0;
  let running = true;
  ch.port1.onmessage = () => {
    const now = performance.now();
    maxGap = Math.max(maxGap, now - last);
    last = now;
    if (running) ch.port2.postMessage(0);
  };
  ch.port2.postMessage(0);
  try {
    await work();
  } finally {
    running = false;
    ch.port1.close();
  }
  return maxGap;
}

function median(xs: number[]): number {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

export async function runBenchmark(
  vault: PixelVault,
  images: { name: string; file: Blob }[],
  cfg: BenchConfig,
  onStatus: (msg: string) => void = () => {},
): Promise<BenchRow[]> {
  const rows: BenchRow[] = [];

  for (const { name, file } of images) {
    const probe = await createImageBitmap(file, { imageOrientation: "from-image" });
    const sourceSize = `${probe.width}×${probe.height}`;
    probe.close();

    // 품질 비교 기준: 각 방식의 "무손실(PNG) 리사이즈" 결과
    onStatus(`${name}: 기준 이미지 준비`);
    const canvasRef = await canvasOnce(file, "png", cfg);
    const canvasRefPixels = canvasRef.ctx.getImageData(0, 0, canvasRef.w, canvasRef.h);
    const wasmRefPixels = await decodeToImageData((await wasmOnce(vault, file, "png", cfg)).blob);

    for (const format of cfg.formats) {
      for (const pipeline of ["canvas", "wasm"] as const) {
        const once = () =>
          pipeline === "canvas" ? canvasOnce(file, format, cfg) : wasmOnce(vault, file, format, cfg);

        onStatus(`${name}: ${format} / ${pipeline} — 워밍업`);
        await once(); // JIT·wasm 최적화 컴파일(tier-up)·캐시를 데우는 1회는 버린다

        const times: number[] = [];
        let last: Awaited<ReturnType<typeof once>> | undefined;
        for (let i = 0; i < cfg.runs; i++) {
          onStatus(`${name}: ${format} / ${pipeline} — ${i + 1}/${cfg.runs}`);
          const t0 = performance.now();
          last = await once();
          times.push(performance.now() - t0);
        }

        onStatus(`${name}: ${format} / ${pipeline} — 메인 스레드 측정`);
        const gap = await measureMainThreadGap(once);

        const out = last!;
        const decoded = await decodeToImageData(out.blob);
        const ref = pipeline === "canvas" ? canvasRefPixels : wasmRefPixels;

        rows.push({
          image: name,
          imageBytes: file.size,
          sourceSize,
          format,
          pipeline,
          actualMime: out.blob.type,
          medianMs: median(times),
          minMs: Math.min(...times),
          outputBytes: out.blob.size,
          width: out.w,
          height: out.h,
          ssim: ssim(decoded, ref),
          psnr: psnr(decoded, ref),
          mainThreadMaxGapMs: gap,
          blob: out.blob,
        });
      }
    }
  }
  onStatus("완료");
  return rows;
}

/** 브라우저 안에서 "사진 같은" 테스트 이미지를 만든다 (crates/core/examples/gen_bench_image.rs 와 같은 아이디어) */
export async function generateSamplePhoto(width = 4032, height = 3024): Promise<File> {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d")!;
  const img = ctx.createImageData(width, height);
  const d = img.data;
  let seed = 12345;
  const rand = () => ((seed = (seed * 1103515245 + 12345) >>> 0) / 0xffffffff);
  for (let y = 0; y < height; y++) {
    const fy = y / height;
    for (let x = 0; x < width; x++) {
      const fx = x / width;
      const texture = 25 * (Math.sin(fx * 90) * Math.cos(fy * 70) + Math.sin(fx * 13 + fy * 17));
      const p = (y * width + x) * 4;
      d[p] = 80 + 120 * fy + texture + (rand() - 0.5) * 24;
      d[p + 1] = 140 + 60 * (1 - fy) + texture + (rand() - 0.5) * 24;
      d[p + 2] = 200 - 120 * fy + texture + (rand() - 0.5) * 24;
      d[p + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
  // 해 (둥근 경계 = 리사이즈 필터 차이가 잘 보이는 곳)
  ctx.fillStyle = "#fad228";
  ctx.beginPath();
  ctx.arc(width * 0.2, height * 0.2, height * 0.1, 0, Math.PI * 2);
  ctx.fill();
  // 가는 선들 (에일리어싱/선명도 비교용)
  ctx.strokeStyle = "#111";
  for (let i = 0; i < 40; i++) {
    ctx.lineWidth = 1 + (i % 3);
    ctx.beginPath();
    ctx.moveTo(width * 0.55 + i * 18, height * 0.45);
    ctx.lineTo(width * 0.6 + i * 18, height * 0.85);
    ctx.stroke();
  }
  const blob = await canvas.convertToBlob({ type: "image/jpeg", quality: 0.92 });
  return new File([blob], `sample-${width}x${height}.jpg`, { type: "image/jpeg" });
}

export function toMarkdown(rows: BenchRow[], env: string): string {
  const fmt = (n: number) => n.toLocaleString("en-US", { maximumFractionDigits: 0 });
  const kb = (n: number) => `${(n / 1024).toFixed(0)} KB`;
  const lines = [
    `| 이미지 | 포맷 | 방식 | 시간 (중앙값) | 결과 크기 | SSIM | 메인 스레드 최대 멈춤 |`,
    `|---|---|---|---:|---:|---:|---:|`,
  ];
  for (const r of rows) {
    const note = r.actualMime !== MIME[r.format] ? ` ⚠️ ${r.actualMime} 로 대체됨` : "";
    lines.push(
      `| ${r.image} (${r.sourceSize}) | ${r.format} | ${r.pipeline === "wasm" ? "**WASM (Worker)**" : "Canvas API"} | ${fmt(r.medianMs)} ms | ${kb(r.outputBytes)}${note} | ${r.ssim.toFixed(4)} | ${fmt(r.mainThreadMaxGapMs)} ms |`,
    );
  }
  lines.push("", `> ${env}`);
  return lines.join("\n");
}
