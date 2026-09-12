"use client";

import { useEffect, useRef, useState } from "react";
import type { OutputFormat } from "@sc0031/pixelvault";
import { formatBytes, getVault } from "@/lib/pixelvault";
import { generateSamplePhoto, runBenchmark, toMarkdown, type BenchConfig, type BenchRow } from "@/lib/bench";
import styles from "./Benchmark.module.css";

declare global {
  interface Window {
    /** 자동화(헤드리스 브라우저)용: 샘플 이미지로 벤치마크를 돌리고 결과를 돌려준다 */
    __pixelvaultBench?: (cfg?: Partial<BenchConfig>) => Promise<{ markdown: string; rows: Omit<BenchRow, "blob">[] }>;
  }
}

const ALL_FORMATS: OutputFormat[] = ["webp", "jpeg", "png"];

function envString() {
  if (typeof navigator === "undefined") return "";
  return `${navigator.userAgent} · ${navigator.hardwareConcurrency} cores · ${new Date().toISOString().slice(0, 10)}`;
}

export default function Benchmark() {
  const [images, setImages] = useState<{ name: string; file: File }[]>([]);
  const [cfg, setCfg] = useState<BenchConfig>({ maxWidth: 1920, quality: 82, formats: ["webp", "jpeg"], runs: 3 });
  const [rows, setRows] = useState<BenchRow[]>([]);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [cropFormat, setCropFormat] = useState<OutputFormat>("webp");
  const inputRef = useRef<HTMLInputElement>(null);

  async function addSample() {
    setBusy(true);
    setStatus("샘플 이미지 생성 중 (4032×3024)…");
    const file = await generateSamplePhoto();
    setImages((prev) => [...prev, { name: file.name, file }]);
    setStatus("");
    setBusy(false);
  }

  async function run(list = images, config = cfg) {
    if (list.length === 0) return [];
    setBusy(true);
    setRows([]);
    try {
      const vault = getVault();
      await vault.warmup();
      const result = await runBenchmark(vault, list, config, setStatus);
      setRows(result);
      return result;
    } catch (e) {
      setStatus(`실패: ${e instanceof Error ? e.message : String(e)}`);
      return [];
    } finally {
      setBusy(false);
    }
  }

  // 헤드리스 자동화 훅 (README 표 생성에 사용: bench/browser-bench.mjs)
  const runRef = useRef(run);
  useEffect(() => {
    runRef.current = run;
  });
  useEffect(() => {
    window.__pixelvaultBench = async (partial = {}) => {
      const file = await generateSamplePhoto();
      const config = { ...cfg, ...partial };
      const result = await runRef.current([{ name: file.name, file }], config);
      return {
        markdown: toMarkdown(result, envString()),
        // Blob 은 직렬화할 수 없으니 빼고 돌려준다
        rows: result.map((r) => {
          const { blob, ...rest } = r;
          void blob;
          return rest;
        }),
      };
    };
    return () => {
      delete window.__pixelvaultBench;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const markdown = rows.length ? toMarkdown(rows, envString()) : "";

  return (
    <section className={styles.wrap}>
      <div className={styles.inputs}>
        <input
          ref={inputRef}
          type="file"
          accept="image/jpeg,image/png,image/webp"
          multiple
          hidden
          onChange={(e) => {
            const files = Array.from(e.target.files ?? []);
            setImages((prev) => [...prev, ...files.map((file) => ({ name: file.name, file }))]);
            e.target.value = "";
          }}
        />
        <button onClick={() => inputRef.current?.click()} disabled={busy}>
          내 사진 추가
        </button>
        <button onClick={addSample} disabled={busy}>
          샘플 사진 생성 (12MP)
        </button>
        {images.length > 0 && (
          <button className={styles.ghost} onClick={() => setImages([])} disabled={busy}>
            비우기
          </button>
        )}
        <span className={styles.muted}>
          {images.map((im) => `${im.name} (${formatBytes(im.file.size)})`).join(", ") || "이미지를 추가하세요"}
        </span>
      </div>

      <div className={styles.controls}>
        <label>
          최대 가로
          <input
            type="number"
            value={cfg.maxWidth}
            min={1}
            onChange={(e) => setCfg({ ...cfg, maxWidth: Number(e.target.value) || 1 })}
          />
        </label>
        <label>
          품질
          <input
            type="number"
            value={cfg.quality}
            min={1}
            max={100}
            onChange={(e) => setCfg({ ...cfg, quality: Math.min(100, Math.max(1, Number(e.target.value) || 1)) })}
          />
        </label>
        <label>
          반복
          <input
            type="number"
            value={cfg.runs}
            min={1}
            max={10}
            onChange={(e) => setCfg({ ...cfg, runs: Math.min(10, Math.max(1, Number(e.target.value) || 1)) })}
          />
        </label>
        {ALL_FORMATS.map((f) => (
          <label key={f} className={styles.check}>
            <input
              type="checkbox"
              checked={cfg.formats.includes(f)}
              onChange={(e) =>
                setCfg({
                  ...cfg,
                  formats: e.target.checked ? [...cfg.formats, f] : cfg.formats.filter((x) => x !== f),
                })
              }
            />
            {f}
          </label>
        ))}
        <button className={styles.primary} onClick={() => run()} disabled={busy || images.length === 0}>
          {busy ? "측정 중…" : "벤치마크 실행"}
        </button>
      </div>

      {status && <p className={styles.muted}>{status}</p>}
      <p className={styles.hint}>
        💡 정확한 측정을 위해 이 탭을 화면 앞에 둔 채로 기다리세요. 가려진 탭은 브라우저가 CPU 우선순위를 낮춥니다.
      </p>

      {rows.length > 0 && (
        <>
          <div className={styles.tableWrap}>
            <table className={styles.table} data-testid="bench-table">
              <thead>
                <tr>
                  <th>이미지</th>
                  <th>포맷</th>
                  <th>방식</th>
                  <th>시간 (중앙값)</th>
                  <th>결과 크기</th>
                  <th title="각 방식의 무손실 리사이즈 결과 대비. 1 = 손실 없음">SSIM</th>
                  <th>PSNR</th>
                  <th title="처리 1회 동안 메인 스레드가 가장 오래 멈춘 시간">메인 스레드 최대 멈춤</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => {
                  const pair = rows.find(
                    (o) => o !== r && o.image === r.image && o.format === r.format,
                  );
                  const better = (a: number, b: number | undefined, lowerIsBetter = true) =>
                    b !== undefined && (lowerIsBetter ? a < b : a > b) ? styles.win : undefined;
                  const fallback = r.actualMime !== `image/${r.format}`;
                  return (
                    <tr key={i} className={i % 2 === 1 ? styles.groupEnd : undefined}>
                      <td>{r.image}</td>
                      <td>{r.format}</td>
                      <td>{r.pipeline === "wasm" ? <strong>WASM (Worker)</strong> : "Canvas API"}</td>
                      <td className={better(r.medianMs, pair?.medianMs)}>{r.medianMs.toFixed(0)} ms</td>
                      <td className={better(r.outputBytes, pair?.outputBytes)}>
                        {formatBytes(r.outputBytes)}
                        {fallback && <span title="브라우저가 이 포맷 인코딩을 지원하지 않아 다른 포맷을 돌려줌"> ⚠️ {r.actualMime}</span>}
                      </td>
                      <td className={better(r.ssim, pair?.ssim, false)}>{r.ssim.toFixed(4)}</td>
                      <td>{Number.isFinite(r.psnr) ? `${r.psnr.toFixed(1)} dB` : "∞"}</td>
                      <td className={better(r.mainThreadMaxGapMs, pair?.mainThreadMaxGapMs)}>
                        {r.mainThreadMaxGapMs.toFixed(0)} ms
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          <div className={styles.cropHeader}>
            <h3>리사이즈 선명도 비교 (3배 확대)</h3>
            <select value={cropFormat} onChange={(e) => setCropFormat(e.target.value as OutputFormat)}>
              {cfg.formats.map((f) => (
                <option key={f}>{f}</option>
              ))}
            </select>
          </div>
          <CropCompare rows={rows.filter((r) => r.image === rows[0].image && r.format === cropFormat)} />

          <details className={styles.md}>
            <summary>README 용 마크다운</summary>
            <pre>{markdown}</pre>
            <button
              onClick={async () => {
                await navigator.clipboard.writeText(markdown);
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }}
            >
              {copied ? "복사됨!" : "복사"}
            </button>
          </details>
        </>
      )}
    </section>
  );
}

/** 두 결과의 같은 영역을 확대해서 나란히 보여 준다. 리사이즈 필터 차이(선명도, 계단 현상)가 보인다. */
function CropCompare({ rows }: { rows: BenchRow[] }) {
  return (
    <div className={styles.crops}>
      {rows.map((r) => (
        <figure key={r.pipeline}>
          <Crop blob={r.blob} />
          <figcaption>
            {r.pipeline === "wasm" ? "WASM (Lanczos3 + SIMD)" : "Canvas API (drawImage, high)"} · {formatBytes(r.outputBytes)}
          </figcaption>
        </figure>
      ))}
    </div>
  );
}

function Crop({ blob }: { blob: Blob }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let cancelled = false;
    createImageBitmap(blob).then((bmp) => {
      const canvas = ref.current;
      if (cancelled || !canvas) return;
      const cw = 160;
      const ch = 110;
      // 샘플 이미지의 가는 선들이 있는 영역 근처
      const sx = Math.max(0, Math.min(bmp.width - cw, Math.round(bmp.width * 0.6)));
      const sy = Math.max(0, Math.min(bmp.height - ch, Math.round(bmp.height * 0.55)));
      canvas.width = cw * 3;
      canvas.height = ch * 3;
      const ctx = canvas.getContext("2d")!;
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(bmp, sx, sy, cw, ch, 0, 0, cw * 3, ch * 3);
      bmp.close();
    });
    return () => {
      cancelled = true;
    };
  }, [blob]);
  return <canvas ref={ref} className={styles.crop} />;
}
