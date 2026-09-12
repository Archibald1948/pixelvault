"use client";

import { useEffect, useRef, useState } from "react";
import {
  formatBytes,
  getVault,
  processImage,
  type OutputFormat,
  type ProcessOptions,
  type ProcessResult,
} from "@/lib/pixelvault";
import BlurhashCanvas from "./BlurhashCanvas";
import MainThreadMonitor from "./MainThreadMonitor";
import styles from "./Converter.module.css";

type Mode = "parallel" | "sequential" | "main";
type Status = "idle" | "pending" | "done" | "error";

interface Item {
  id: string;
  file: File;
  srcUrl: string;
  status: Status;
  result?: ProcessResult & { url: string };
  error?: string;
}

const MODE_LABEL: Record<Mode, string> = {
  parallel: "Worker 병렬",
  sequential: "Worker 순차",
  main: "메인 스레드 (비교용)",
};

export default function Converter() {
  const [items, setItems] = useState<Item[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [format, setFormat] = useState<OutputFormat>("webp");
  const [maxWidth, setMaxWidth] = useState(1920);
  const [quality, setQuality] = useState(82);
  const [stripExif, setStripExif] = useState(true);
  const [wantBlurhash, setWantBlurhash] = useState(true);
  const [mode, setMode] = useState<Mode>("parallel");
  const [workers, setWorkers] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [wallMs, setWallMs] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const itemsRef = useRef(items);
  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  // 워커를 미리 띄워 wasm 을 받아 둔다 (첫 변환 대기 줄이기)
  useEffect(() => {
    const vault = getVault();
    vault
      .warmup()
      .catch(() => {})
      .finally(() => setWorkers(vault.workers));
  }, []);

  // 언마운트 시 object URL 정리
  useEffect(() => () => itemsRef.current.forEach(revokeItem), []);

  function onFiles(list: FileList | null | undefined) {
    const files = Array.from(list ?? []).filter((f) => /^image\/(jpeg|png|webp)$/.test(f.type));
    if (files.length === 0) return;
    items.forEach(revokeItem);
    const next = files.map<Item>((file, i) => ({
      id: `${Date.now()}-${i}-${file.name}`,
      file,
      srcUrl: URL.createObjectURL(file),
      status: "idle",
    }));
    setItems(next);
    setSelectedId(next[0].id);
    setProgress({ done: 0, total: next.length });
    setWallMs(null);
  }

  function update(id: string, patch: Partial<Item>) {
    setItems((prev) => prev.map((it) => (it.id === id ? { ...it, ...patch } : it)));
  }

  async function run() {
    if (items.length === 0 || busy) return;
    const options: ProcessOptions = {
      format,
      maxWidth: maxWidth > 0 ? maxWidth : undefined,
      quality,
      stripExif,
      blurhash: wantBlurhash,
    };

    // 이전 결과 정리
    items.forEach((it) => it.result && URL.revokeObjectURL(it.result.url));
    setItems((prev) => prev.map((it) => ({ ...it, status: "pending", result: undefined, error: undefined })));
    setProgress({ done: 0, total: items.length });
    setWallMs(null);
    setBusy(true);
    const snapshot = items;
    const t0 = performance.now();

    const finish = (item: Item, r: ProcessResult | Error) => {
      if (r instanceof Error) {
        update(item.id, { status: "error", error: r.message });
      } else {
        const url = URL.createObjectURL(new Blob([r.bytes as BlobPart], { type: r.mimeType }));
        update(item.id, { status: "done", result: { ...r, url } });
      }
      setProgress((p) => ({ ...p, done: p.done + 1 }));
    };

    try {
      if (mode === "main") {
        // 비교용: 워커 없이 메인 스레드에서 직접 처리 → 모니터의 공이 멈추는 걸 볼 수 있다
        for (const item of snapshot) {
          await nextFrame();
          try {
            finish(item, await processImage(item.file, options));
          } catch (e) {
            finish(item, toError(e));
          }
        }
      } else {
        const vault = getVault();
        await vault.processMany(
          snapshot.map((it) => it.file), // File 을 그대로 넘긴다 → 읽기도 워커에서
          options,
          {
            concurrency: mode === "sequential" ? 1 : vault.workers,
            onProgress: ({ item }) =>
              finish(snapshot[item.index], item.ok ? item.result : item.error),
          },
        );
      }
    } finally {
      setWallMs(performance.now() - t0);
      setBusy(false);
    }
  }

  const selected = items.find((it) => it.id === selectedId) ?? items[0];
  const totals = items.reduce(
    (acc, it) => {
      if (it.result) {
        acc.original += it.result.originalBytes;
        acc.output += it.result.outputBytes;
      }
      return acc;
    },
    { original: 0, output: 0 },
  );

  return (
    <section className={styles.wrap}>
      <div
        className={styles.drop}
        onClick={() => inputRef.current?.click()}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          onFiles(e.dataTransfer.files);
        }}
      >
        <input
          ref={inputRef}
          type="file"
          accept="image/jpeg,image/png,image/webp"
          multiple
          hidden
          data-testid="file-input"
          onChange={(e) => onFiles(e.target.files)}
        />
        {items.length > 0 ? (
          <span>
            <strong>{items.length}개 파일</strong> ·{" "}
            {formatBytes(items.reduce((s, it) => s + it.file.size, 0))} — 다시 선택하려면 클릭
          </span>
        ) : (
          <span>JPEG / PNG / WebP 파일(여러 개 가능)을 끌어다 놓거나 클릭해서 선택</span>
        )}
      </div>

      <div className={styles.controls}>
        <label>
          포맷
          <select value={format} onChange={(e) => setFormat(e.target.value as OutputFormat)}>
            <option value="webp">WebP</option>
            <option value="jpeg">JPEG</option>
            <option value="png">PNG</option>
          </select>
        </label>
        <label>
          최대 가로(px)
          <input
            type="number"
            min={0}
            step={10}
            value={maxWidth}
            onChange={(e) => setMaxWidth(Number(e.target.value))}
          />
        </label>
        <label>
          품질 {format === "png" ? "(PNG 는 무손실)" : quality}
          <input
            type="range"
            min={1}
            max={100}
            value={quality}
            disabled={format === "png"}
            onChange={(e) => setQuality(Number(e.target.value))}
          />
        </label>
        <label className={styles.check}>
          <input type="checkbox" checked={stripExif} onChange={(e) => setStripExif(e.target.checked)} />
          EXIF·GPS 제거
        </label>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={wantBlurhash}
            onChange={(e) => setWantBlurhash(e.target.checked)}
          />
          BlurHash
        </label>
      </div>

      <div className={styles.controls}>
        <label>
          실행 방식
          <select value={mode} onChange={(e) => setMode(e.target.value as Mode)} data-testid="mode">
            {(Object.keys(MODE_LABEL) as Mode[]).map((m) => (
              <option key={m} value={m}>
                {MODE_LABEL[m]}
                {m === "parallel" && workers ? ` (${workers} workers)` : ""}
              </option>
            ))}
          </select>
        </label>
        <button onClick={run} disabled={items.length === 0 || busy} data-testid="run">
          {busy ? `변환 중… ${progress.done}/${progress.total}` : "변환"}
        </button>
        {progress.total > 0 && (
          <div className={styles.progress} aria-label="진행률">
            <div style={{ width: `${(progress.done / progress.total) * 100}%` }} />
          </div>
        )}
      </div>

      <MainThreadMonitor measuring={busy} />

      {wallMs !== null && totals.original > 0 && (
        <div className={styles.stats} data-testid="stats">
          <Stat label="전체 원본" value={formatBytes(totals.original)} />
          <Stat label="전체 결과" value={formatBytes(totals.output)} />
          <Stat label="절감" value={`${((1 - totals.output / totals.original) * 100).toFixed(1)}%`} accent />
          <Stat label={`전체 소요 (${MODE_LABEL[mode]})`} value={`${wallMs.toFixed(0)} ms`} />
        </div>
      )}

      {items.length > 0 && (
        <div className={styles.tableWrap}>
          <table className={styles.table} data-testid="items">
            <thead>
              <tr>
                <th>파일</th>
                <th>원본</th>
                <th>결과</th>
                <th>절감</th>
                <th>크기</th>
                <th>처리</th>
                <th>메타데이터</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {items.map((it) => (
                <tr
                  key={it.id}
                  onClick={() => setSelectedId(it.id)}
                  className={it.id === selected?.id ? styles.selectedRow : undefined}
                >
                  <td className={styles.name}>{it.file.name}</td>
                  <td>{formatBytes(it.file.size)}</td>
                  <td>{it.result ? formatBytes(it.result.outputBytes) : statusText(it)}</td>
                  <td className={styles.accent}>
                    {it.result && `${((1 - it.result.outputBytes / it.result.originalBytes) * 100).toFixed(0)}%`}
                  </td>
                  <td>{it.result && `${it.result.width}×${it.result.height}`}</td>
                  <td>{it.result && `${it.result.elapsedMs.toFixed(0)} ms`}</td>
                  <td className={styles.meta}>
                    {it.result && it.result.sourceOrientation !== 1 && <span title="EXIF Orientation 보정">↻</span>}
                    {it.result?.hadGps && !it.result.exifKept && <span title="GPS 제거됨">🛰✕</span>}
                    {it.result?.hadGps && it.result.exifKept && <span title="GPS 남아 있음">⚠️</span>}
                  </td>
                  <td>
                    {it.result && (
                      <a
                        href={it.result.url}
                        download={outputName(it.file.name, it.result.format)}
                        onClick={(e) => e.stopPropagation()}
                      >
                        저장
                      </a>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {selected && (
        <>
          {selected.result && (
            <div className={styles.badges} data-testid="badges">
              {selected.result.sourceOrientation !== 1 && (
                <span>↻ 방향 보정됨 (EXIF Orientation {selected.result.sourceOrientation})</span>
              )}
              {selected.result.hadGps && !selected.result.exifKept && <span>🛰 GPS 위치 정보 제거됨</span>}
              {selected.result.hadGps && selected.result.exifKept && (
                <span className={styles.warn}>⚠️ GPS 위치 정보가 결과에 남아 있음</span>
              )}
              {selected.result.blurhash && (
                <span>
                  BlurHash <code>{selected.result.blurhash}</code>
                </span>
              )}
            </div>
          )}
          <div className={styles.previews}>
            <figure>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src={selected.srcUrl} alt="원본" />
              <figcaption>원본 · {selected.file.name}</figcaption>
            </figure>
            <figure>
              {selected.result ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img src={selected.result.url} alt="결과" />
              ) : (
                <div className={styles.placeholder}>
                  {selected.error ? `⚠️ ${selected.error}` : "변환 결과가 여기에 표시됩니다"}
                </div>
              )}
              <figcaption>결과 {selected.result && `(${selected.result.format})`}</figcaption>
            </figure>
            {selected.result?.blurhash && (
              <figure>
                <div className={styles.blurhashBox}>
                  <BlurhashCanvas
                    hash={selected.result.blurhash}
                    aspect={selected.result.width / selected.result.height}
                    className={styles.blurhash}
                  />
                </div>
                <figcaption>BlurHash 플레이스홀더 ({selected.result.blurhash.length}자)</figcaption>
              </figure>
            )}
          </div>
        </>
      )}
    </section>
  );
}

function Stat({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className={styles.stat}>
      <span>{label}</span>
      <strong className={accent ? styles.accent : undefined}>{value}</strong>
    </div>
  );
}

function statusText(it: Item) {
  switch (it.status) {
    case "pending":
      return "처리 중…";
    case "error":
      return <span title={it.error}>⚠️ 실패</span>;
    default:
      return "—";
  }
}

function revokeItem(it: Item) {
  URL.revokeObjectURL(it.srcUrl);
  if (it.result) URL.revokeObjectURL(it.result.url);
}

function outputName(name: string, format: string) {
  const base = name.replace(/\.[^.]+$/, "");
  return `${base}.pixelvault.${format === "jpeg" ? "jpg" : format}`;
}

function toError(e: unknown): Error {
  return e instanceof Error ? e : new Error(String(e));
}

/** 화면이 한 번 그려질 기회를 준다. 백그라운드 탭에서 rAF 가 멈춰도 풀리도록 setTimeout 병행. */
function nextFrame() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => setTimeout(resolve, 0));
    setTimeout(resolve, 100);
  });
}
