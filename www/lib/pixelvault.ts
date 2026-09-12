// 데모 앱에서 쓰는 pixelvault 헬퍼. 실제 로직은 packages/pixelvault (npm 패키지) 에 있다.
import { createPixelVault, type PixelVault } from "@archibald1948/pixelvault";

export {
  decodeBlurhash,
  loadWasm,
  processImage,
  type BatchItem,
  type OutputFormat,
  type ProcessOptions,
  type ProcessResult,
} from "@archibald1948/pixelvault";

let vault: PixelVault | null = null;

/** 앱 전체에서 워커 풀 하나를 공유한다 (브라우저에서만 호출할 것). */
export function getVault(): PixelVault {
  vault ??= createPixelVault();
  return vault;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
