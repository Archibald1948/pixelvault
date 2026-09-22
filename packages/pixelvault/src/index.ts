/**
 * pixelvault — 브라우저 안에서 끝나는 이미지 처리 파이프라인 (Rust → WebAssembly)
 */
import * as Comlink from "comlink";
import { decodeBlurhashSync, loadWasm, processBytes, toBytes } from "./core.js";
import { WorkerPool } from "./pool.js";
import type { ImageInput, ProcessOptions, ProcessResult } from "./types.js";

export type { ImageInput, OutputFormat, ProcessOptions, ProcessResult } from "./types.js";
export { loadWasm };

// ── 메인 스레드 API (스펙의 기본 형태) ────────────────────────────────────

export async function processImage(input: ImageInput, options: ProcessOptions): Promise<ProcessResult> {
  await loadWasm();
  return processBytes(await toBytes(input), options);
}

/** BlurHash 문자열 → RGBA 픽셀 (`new ImageData(pixels, width, height)` 로 캔버스에 그린다) */
export async function decodeBlurhash(
  hash: string,
  width: number,
  height: number,
): Promise<Uint8ClampedArray<ArrayBuffer>> {
  await loadWasm();
  return decodeBlurhashSync(hash, width, height);
}

// ── 워커 풀 API ─────────────────────────────────────────────────────────

export interface PixelVaultOptions {
  /**
   * 워커 개수. 기본 = min(4, CPU 코어 − 1). 워커마다 wasm 메모리를 따로 쓰므로(큰 사진 1장 ≈ 수백 MB 피크)
   * 너무 크게 잡지 않는다.
   */
  workers?: number;
}

export interface ProcessCallOptions {
  /**
   * `Uint8Array`/`ArrayBuffer` 입력을 워커로 **이동(transfer)** 할지. true 면 복사 없이 넘어가는 대신
   * 호출한 쪽의 버퍼가 비워진다(detached). 기본 false(복사). `Blob`/`File` 은 원래 복사가 없다.
   */
  transfer?: boolean;
}

export type BatchItem =
  | { ok: true; index: number; result: ProcessResult }
  | { ok: false; index: number; error: Error };

export interface BatchProgress {
  /** 방금 끝난 항목 */
  item: BatchItem;
  /** 끝난 개수 (성공 + 실패) */
  done: number;
  total: number;
}

export interface BatchOptions {
  /** 동시에 처리할 개수. 1 = 순차 처리. 기본 = 워커 개수. */
  concurrency?: number;
  onProgress?: (progress: BatchProgress) => void;
  /** 중단 신호. 이미 시작한 항목은 끝까지 가고, 아직 시작 안 한 항목은 건너뛴다. */
  signal?: AbortSignal;
}

export class PixelVault {
  #pool: WorkerPool;

  constructor(options: PixelVaultOptions = {}) {
    if (typeof Worker === "undefined") {
      throw new Error("PixelVault needs Web Workers (use processImage() outside the browser)");
    }
    this.#pool = new WorkerPool(options.workers ?? defaultWorkerCount());
  }

  get workers(): number {
    return this.#pool.size;
  }

  /** 워커를 미리 띄우고 wasm 을 로드한다. 안 불러도 첫 작업 때 자동으로 된다. */
  warmup(): Promise<void> {
    return this.#pool.warmup();
  }

  /** 이미지 한 장을 워커에서 처리한다. 메인 스레드는 기다리는 동안 자유롭다. */
  process(input: ImageInput, options: ProcessOptions, call: ProcessCallOptions = {}): Promise<ProcessResult> {
    return this.#pool.run((api) => {
      if (call.transfer && !(input instanceof Blob)) {
        const buffer = input instanceof ArrayBuffer ? input : input.buffer;
        return api.process(Comlink.transfer(input, [buffer as ArrayBuffer]), options);
      }
      return api.process(input, options);
    });
  }

  /**
   * 여러 장을 처리한다. 결과는 **입력 순서대로** 돌려주고, 하나가 실패해도 나머지는 계속한다.
   * 진행 상황은 `onProgress` 로 한 장 끝날 때마다 알려 준다(끝나는 순서는 입력 순서와 다를 수 있음).
   */
  async processMany(
    inputs: readonly ImageInput[],
    options: ProcessOptions,
    batch: BatchOptions = {},
  ): Promise<BatchItem[]> {
    const total = inputs.length;
    const results = new Array<BatchItem>(total);
    const concurrency = Math.max(1, Math.min(batch.concurrency ?? this.workers, total));
    let next = 0;
    let done = 0;

    // "러너" concurrency 개가 공유 카운터에서 다음 인덱스를 하나씩 가져가며 일한다.
    const runner = async () => {
      while (next < total) {
        const index = next++;
        let item: BatchItem;
        if (batch.signal?.aborted) {
          item = { ok: false, index, error: new DOMException("Aborted", "AbortError") };
        } else {
          try {
            item = { ok: true, index, result: await this.process(inputs[index], options) };
          } catch (e) {
            item = { ok: false, index, error: e instanceof Error ? e : new Error(String(e)) };
          }
        }
        results[index] = item;
        done++;
        batch.onProgress?.({ item, done, total });
      }
    };

    await Promise.all(Array.from({ length: concurrency }, runner));
    return results;
  }

  /** BlurHash 디코딩도 워커에서 (작업이라 보통은 메인 스레드 decodeBlurhash 로 충분) */
  decodeBlurhash(hash: string, width: number, height: number): Promise<Uint8ClampedArray> {
    return this.#pool.run((api) => api.decodeBlurhash(hash, width, height));
  }

  /** 워커를 모두 종료한다. 대기 중인 작업은 에러로 끝난다. */
  terminate(): void {
    this.#pool.terminate();
  }
}

export function createPixelVault(options?: PixelVaultOptions): PixelVault {
  return new PixelVault(options);
}

function defaultWorkerCount(): number {
  const cores = typeof navigator !== "undefined" ? navigator.hardwareConcurrency || 2 : 2;
  return Math.max(1, Math.min(4, cores - 1));
}
