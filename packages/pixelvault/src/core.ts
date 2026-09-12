// 메인 스레드와 워커가 함께 쓰는 부분: wasm 로드 + 옵션 검증/변환 + 결과 정리.
import init, {
  decodeBlurhash as rawDecodeBlurhash,
  processImageRaw,
  type InitInput,
} from "./wasm/pixelvault.js";
import type { ImageInput, OutputFormat, ProcessOptions, ProcessResult } from "./types.js";

let ready: Promise<void> | null = null;

/**
 * wasm 을 로드한다. 여러 번 불러도 한 번만 로드된다.
 * 인자를 생략하면 패키지 안의 .wasm 을 `new URL(..., import.meta.url)` 로 찾는다(번들러가 자동 처리).
 */
export function loadWasm(source?: InitInput | Promise<InitInput>): Promise<void> {
  ready ??= init(source === undefined ? undefined : { module_or_path: source }).then(() => undefined);
  return ready;
}

export async function toBytes(input: ImageInput): Promise<Uint8Array> {
  if (input instanceof Uint8Array) return input;
  if (input instanceof ArrayBuffer) return new Uint8Array(input);
  return new Uint8Array(await input.arrayBuffer());
}

const FORMATS: readonly OutputFormat[] = ["webp", "jpeg", "png"];

/**
 * JS number → Rust u8/u32 로 넘기기 전에 검증한다.
 *
 * wasm-bindgen 은 범위를 검사하지 않고 비트를 잘라서 넘긴다. 예를 들어 quality 300 은
 * u8 로 잘려서 44 가 되고, maxWidth -1 은 u32 로 4294967295 가 된다. 조용히 틀린 결과가 나오는 것보다
 * 여기서 명확한 에러를 던지는 게 낫다.
 */
function validate(o: ProcessOptions): void {
  if (!FORMATS.includes(o.format)) {
    throw new RangeError(`format must be one of ${FORMATS.join(", ")}, got ${String(o.format)}`);
  }
  if (o.quality !== undefined && !(Number.isInteger(o.quality) && o.quality >= 1 && o.quality <= 100)) {
    throw new RangeError(`quality must be an integer 1-100, got ${o.quality}`);
  }
  for (const key of ["maxWidth", "maxHeight"] as const) {
    const v = o[key];
    if (v !== undefined && !(Number.isInteger(v) && v >= 1 && v <= 0xffff_ffff)) {
      throw new RangeError(`${key} must be a positive integer, got ${v}`);
    }
  }
}

/** 동기 처리. 호출한 스레드를 막는다(워커 안에서 쓰거나, 작은 이미지일 때만). */
export function processBytes(bytes: Uint8Array, options: ProcessOptions): ProcessResult {
  validate(options);

  // wasm32-unknown-unknown 에는 시계가 없어서(std::time::Instant 가 panic) JS 쪽에서 잰다.
  const t0 = performance.now();
  const raw = processImageRaw(
    bytes,
    options.maxWidth,
    options.maxHeight,
    options.format,
    options.quality ?? 82,
    options.stripExif ?? true,
    options.blurhash ?? false,
  );
  const elapsedMs = performance.now() - t0;

  try {
    return {
      bytes: raw.takeBytes(),
      width: raw.width,
      height: raw.height,
      format: raw.format as OutputFormat,
      mimeType: raw.mimeType,
      originalBytes: raw.originalBytes,
      outputBytes: raw.outputBytes,
      blurhash: raw.blurhash,
      elapsedMs,
      sourceOrientation: raw.sourceOrientation,
      hadGps: raw.hadGps,
      exifKept: raw.exifKept,
    };
  } finally {
    // RawProcessResult 는 wasm 메모리에 사는 Rust 구조체. JS GC 가 모르므로 직접 해제한다.
    raw.free();
  }
}

export function decodeBlurhashSync(hash: string, width: number, height: number): Uint8ClampedArray<ArrayBuffer> {
  const rgba = rawDecodeBlurhash(hash, width, height);
  return new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength);
}
