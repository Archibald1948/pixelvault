| 이미지 | 포맷 | 방식 | 시간 (중앙값) | 결과 크기 | SSIM | 메인 스레드 최대 멈춤 |
|---|---|---|---:|---:|---:|---:|
| sample-4032x3024.jpg (4032×3024) | webp | Canvas API | 238 ms | 87 KB | 0.9609 | 40 ms |
| sample-4032x3024.jpg (4032×3024) | webp | **WASM (Worker)** | 476 ms | 103 KB | 0.9409 | 4 ms |
| sample-4032x3024.jpg (4032×3024) | jpeg | Canvas API | 98 ms | 201 KB | 0.9696 | 38 ms |
| sample-4032x3024.jpg (4032×3024) | jpeg | **WASM (Worker)** | 141 ms | 230 KB | 0.9534 | 1 ms |
| sample-4032x3024.jpg (4032×3024) | png | Canvas API | 168 ms | 4996 KB | 1.0000 | 93 ms |
| sample-4032x3024.jpg (4032×3024) | png | **WASM (Worker)** | 757 ms | 3871 KB | 1.0000 | 5 ms |

> Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/152.0.0.0 Safari/537.36 · 8 cores · 2026-09-11

```json
[
  {
    "image": "sample-4032x3024.jpg",
    "imageBytes": 3313254,
    "sourceSize": "4032×3024",
    "format": "webp",
    "pipeline": "canvas",
    "actualMime": "image/webp",
    "medianMs": 237.5,
    "minMs": 232.69999999925494,
    "outputBytes": 88946,
    "width": 1920,
    "height": 1440,
    "ssim": 0.9608903068619545,
    "psnr": 40.16575372506206,
    "mainThreadMaxGapMs": 39.5
  },
  {
    "image": "sample-4032x3024.jpg",
    "imageBytes": 3313254,
    "sourceSize": "4032×3024",
    "format": "webp",
    "pipeline": "wasm",
    "actualMime": "image/webp",
    "medianMs": 476.19999999925494,
    "minMs": 474.30000000074506,
    "outputBytes": 104972,
    "width": 1920,
    "height": 1440,
    "ssim": 0.9408523139017262,
    "psnr": 38.7529428847955,
    "mainThreadMaxGapMs": 4.300000000745058
  },
  {
    "image": "sample-4032x3024.jpg",
    "imageBytes": 3313254,
    "sourceSize": "4032×3024",
    "format": "jpeg",
    "pipeline": "canvas",
    "actualMime": "image/jpeg",
    "medianMs": 97.69999999925494,
    "minMs": 95.59999999776483,
    "outputBytes": 206026,
    "width": 1920,
    "height": 1440,
    "ssim": 0.9696487221089796,
    "psnr": 40.48804728909511,
    "mainThreadMaxGapMs": 37.599999997764826
  },
  {
    "image": "sample-4032x3024.jpg",
    "imageBytes": 3313254,
    "sourceSize": "4032×3024",
    "format": "jpeg",
    "pipeline": "wasm",
    "actualMime": "image/jpeg",
    "medianMs": 141.30000000074506,
    "minMs": 140.5,
    "outputBytes": 235189,
    "width": 1920,
    "height": 1440,
    "ssim": 0.953351014058257,
    "psnr": 38.93616710775194,
    "mainThreadMaxGapMs": 0.7000000029802322
  },
  {
    "image": "sample-4032x3024.jpg",
    "imageBytes": 3313254,
    "sourceSize": "4032×3024",
    "format": "png",
    "pipeline": "canvas",
    "actualMime": "image/png",
    "medianMs": 167.80000000074506,
    "minMs": 164.5,
    "outputBytes": 5116021,
    "width": 1920,
    "height": 1440,
    "ssim": 1,
    "psnr": null,
    "mainThreadMaxGapMs": 93.09999999776483
  },
  {
    "image": "sample-4032x3024.jpg",
    "imageBytes": 3313254,
    "sourceSize": "4032×3024",
    "format": "png",
    "pipeline": "wasm",
    "actualMime": "image/png",
    "medianMs": 757.1000000014901,
    "minMs": 751.6999999992549,
    "outputBytes": 3964004,
    "width": 1920,
    "height": 1440,
    "ssim": 1,
    "psnr": null,
    "mainThreadMaxGapMs": 5.099999997764826
  }
]
```
