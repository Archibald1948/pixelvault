// Web Worker 진입점. comlink 가 postMessage 기반 RPC 를 "그냥 async 함수 호출"처럼 보이게 해 준다.
import * as Comlink from "comlink";
import { decodeBlurhashSync, loadWasm, processBytes, toBytes } from "./core.js";
import type { ImageInput, ProcessOptions, ProcessResult } from "./types.js";

const api = {
  async process(input: ImageInput, options: ProcessOptions): Promise<ProcessResult> {
    await loadWasm();
    // File/Blob 이면 여기(워커 스레드)에서 읽는다. 10MB 파일을 읽는 시간도 메인 스레드와 무관.
    const bytes = await toBytes(input);
    const result = processBytes(bytes, options);
    // 결과 버퍼의 "소유권"을 메인 스레드로 넘긴다(transfer). 복사 없이 포인터만 옮겨 가고,
    // 이 워커 쪽 result.bytes 는 길이 0 으로 비워진다(detached).
    return Comlink.transfer(result, [result.bytes.buffer]);
  },

  async decodeBlurhash(hash: string, width: number, height: number): Promise<Uint8ClampedArray> {
    await loadWasm();
    const rgba = decodeBlurhashSync(hash, width, height);
    return Comlink.transfer(rgba, [rgba.buffer]);
  },

  /** 워커가 뜨자마자 wasm 을 미리 로드해 두고 싶을 때 */
  async warmup(): Promise<void> {
    await loadWasm();
  },
};

export type WorkerApi = typeof api;

Comlink.expose(api);
