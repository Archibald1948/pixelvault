export type OutputFormat = "webp" | "jpeg" | "png";

/** 처리할 입력. File/Blob 을 넘기면 워커 안에서 읽으므로 메인 스레드가 바이트를 만지지 않는다. */
export type ImageInput = Uint8Array | ArrayBuffer | Blob;

export interface ProcessOptions {
  /** 결과의 최대 가로(px). 원본보다 크면 확대하지 않는다. */
  maxWidth?: number;
  /** 결과의 최대 세로(px). */
  maxHeight?: number;
  format: OutputFormat;
  /** 1-100, 기본 82. PNG 는 무시. */
  quality?: number;
  /** EXIF(촬영 정보·GPS) 제거. 기본 true. 방향(Orientation)은 제거 전에 픽셀에 반영된다. */
  stripExif?: boolean;
  /** BlurHash 문자열 생성. 기본 false. */
  blurhash?: boolean;
}

export interface ProcessResult {
  bytes: Uint8Array;
  /** 방향 보정 후(화면에 보이는) 크기 */
  width: number;
  height: number;
  format: OutputFormat;
  mimeType: string;
  originalBytes: number;
  outputBytes: number;
  blurhash?: string;
  /** wasm 처리 시간(ms). 파일 읽기·스레드 간 전송 시간은 제외. */
  elapsedMs: number;
  /** 원본 EXIF Orientation (1-8). 1 이 아니면 픽셀에 회전을 반영했다는 뜻. */
  sourceOrientation: number;
  /** 원본에 GPS 위치 정보가 있었는지 */
  hadGps: boolean;
  /** 결과 파일에 EXIF 가 남아 있는지 (stripExif: false 일 때만 true 가 될 수 있음) */
  exifKept: boolean;
}
