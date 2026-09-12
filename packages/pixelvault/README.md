# @sc0031/pixelvault

Resize, convert, strip EXIF and generate BlurHash placeholders **entirely in the browser** — no upload, no server.
The pipeline is written in Rust, compiled to WebAssembly, and runs in a pool of Web Workers so your UI never freezes.

- **Formats**: JPEG / PNG / WebP in → WebP (lossy, libwebp) / JPEG (4:2:0, optimized Huffman) / PNG out
- **Resize**: Lanczos3 with WebAssembly SIMD, aspect ratio preserved, never upscales
- **Privacy**: EXIF (GPS, camera, timestamps) removed by default
- **Orientation**: EXIF orientation is applied to the pixels *before* stripping — iPhone portraits stay upright
- **Color**: ICC profiles (e.g. Display P3) are preserved
- **BlurHash**: optional placeholder string, plus a decoder for rendering it
- **Off the main thread**: `File` objects are read inside the worker; results come back as transferred buffers (zero-copy)
- **Consistent**: byte-identical output in every browser (WebP encoding works in Safari too)

~410 KB of WebAssembly (gzip), loaded lazily on first use.

## Install

```bash
npm install @sc0031/pixelvault
```

## Usage

```ts
import { createPixelVault } from "@sc0031/pixelvault";

const vault = createPixelVault(); // spawns up to 4 workers lazily

const result = await vault.process(file, {
  format: "webp",
  maxWidth: 1920,
  quality: 82,     // 1-100, default 82
  stripExif: true, // default true
  blurhash: true,  // default false
});

const blob = new Blob([result.bytes], { type: result.mimeType });
console.log(result.width, result.height, result.outputBytes, result.blurhash);
```

### Many files, with progress

```ts
const items = await vault.processMany(files, { format: "webp", maxWidth: 1920 }, {
  concurrency: vault.workers, // 1 = sequential
  onProgress: ({ done, total, item }) => {
    console.log(`${done}/${total}`, item.ok ? item.result.outputBytes : item.error);
  },
  signal: abortController.signal,
});
// `items` is in input order; a failed file does not stop the others.
```

### Main-thread API

```ts
import { processImage } from "@sc0031/pixelvault";

// Blocks the calling thread while processing — fine inside your own worker, or for small images.
const result = await processImage(bytesOrBlob, { format: "jpeg", maxWidth: 800 });
```

### Rendering a BlurHash

```ts
import { decodeBlurhash } from "@sc0031/pixelvault";

const pixels = await decodeBlurhash(result.blurhash!, 32, 24);
canvas.getContext("2d")!.putImageData(new ImageData(pixels, 32, 24), 0, 0);
```

## API

### `ProcessOptions`

| Option | Type | Default | |
|---|---|---|---|
| `format` | `"webp" \| "jpeg" \| "png"` | — | Output format |
| `maxWidth` | `number` | — | Max output width (displayed orientation). Never upscales. |
| `maxHeight` | `number` | — | Max output height |
| `quality` | `number` | `82` | 1–100. Ignored for PNG. |
| `stripExif` | `boolean` | `true` | Remove EXIF. When `false`, EXIF is kept but its orientation tag is reset to 1 (pixels are already rotated). |
| `blurhash` | `boolean` | `false` | Also compute a BlurHash string |

### `ProcessResult`

| Field | Type | |
|---|---|---|
| `bytes` | `Uint8Array` | Encoded output |
| `width`, `height` | `number` | Output size after orientation |
| `format`, `mimeType` | `string` | |
| `originalBytes`, `outputBytes` | `number` | |
| `blurhash` | `string \| undefined` | |
| `elapsedMs` | `number` | Time spent in WebAssembly |
| `sourceOrientation` | `number` | EXIF orientation of the input (1–8) |
| `hadGps` | `boolean` | The input contained GPS data |
| `exifKept` | `boolean` | The output contains EXIF |

### `createPixelVault({ workers? })` → `PixelVault`

- `process(input, options, { transfer? })` — `input` is a `Blob`/`File`, `Uint8Array` or `ArrayBuffer`. With `transfer: true`, typed-array input is moved to the worker instead of copied (the caller's buffer becomes detached).
- `processMany(inputs, options, { concurrency?, onProgress?, signal? })`
- `warmup()` — spawn workers and compile the wasm ahead of time
- `terminate()`

## Bundlers

The package uses the standard `new Worker(new URL("./worker.js", import.meta.url))` and `new URL("./pixelvault_bg.wasm", import.meta.url)` patterns, which bundlers understand without configuration. Tested with **Next.js 16 (Turbopack)** and **Vite 8** (dev server and production build).

- **Older Vite versions**: if the worker or `.wasm` fails to load in the dev server, exclude the package from dependency pre-bundling so the `import.meta.url` references survive:
  ```ts
  export default defineConfig({ optimizeDeps: { exclude: ["@sc0031/pixelvault"] } });
  ```
- **Content Security Policy**: WebAssembly needs `script-src 'wasm-unsafe-eval'`, and workers need `worker-src 'self'`.
- No `SharedArrayBuffer`, so **no COOP/COEP headers** are required.

## Benchmarks

12 MP JPEG → 1920 px, quality 82, headless Chrome 152 on Apple Silicon, median of 5 runs:

| Format | Pipeline | Time | Size | Main-thread max stall |
|---|---|---:|---:|---:|
| WebP | Canvas API | 238 ms | 87 KB | 40 ms |
| WebP | **pixelvault** | 476 ms | 103 KB | **4 ms** |
| JPEG | Canvas API | 98 ms | 201 KB | 38 ms |
| JPEG | **pixelvault** | 141 ms | 230 KB | **1 ms** |
| PNG | Canvas API | 168 ms | 4,996 KB | 93 ms |
| PNG | **pixelvault** | 757 ms | **3,871 KB** | **5 ms** |

Chrome's native codecs are faster in raw throughput; pixelvault keeps the main thread free and gives identical results in every browser. Size differences come from the resize filter (Lanczos3 keeps more detail), not the encoders — given identical pixels, both produce the same sizes. Full methodology: [docs/m4-benchmark.md](https://github.com/Archibald1948/pixelvault/blob/main/docs/m4-benchmark.md).

## Browser support

Requires WebAssembly SIMD and module workers: Chrome/Edge 91+, Firefox 114+, Safari 16.4+.

## Limits

- Decoded image ≤ 1 GiB (checked from the header, before decoding)
- WebP output ≤ 16383 px per side, JPEG ≤ 65535 px
- Each worker holds its own wasm memory (a 12 MP photo peaks at a few hundred MB), hence the default cap of 4 workers

## License

MIT. The WebAssembly binary includes libwebp (BSD-3-Clause), kamadak-exif (BSD-2-Clause) and code based in part on the work of the Independent JPEG Group — see [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
