// 이미지 품질 지표 — SSIM, PSNR. 벤치마크 페이지 전용.
//
// SSIM(Structural Similarity): 두 이미지의 밝기·대비·구조가 얼마나 비슷한지 0~1 로 나타낸다(1 = 동일).
// PSNR 보다 사람 눈의 판단과 잘 맞는다. 여기서는 휘도(Y) 채널에 8×8 창을 4px 간격으로 밀면서 계산한다.

function luma(img: ImageData): Float32Array {
  const { data, width, height } = img;
  const out = new Float32Array(width * height);
  for (let i = 0, p = 0; i < out.length; i++, p += 4) {
    // BT.601 휘도
    out[i] = 0.299 * data[p] + 0.587 * data[p + 1] + 0.114 * data[p + 2];
  }
  return out;
}

export function ssim(a: ImageData, b: ImageData): number {
  if (a.width !== b.width || a.height !== b.height) {
    throw new Error(`size mismatch: ${a.width}x${a.height} vs ${b.width}x${b.height}`);
  }
  const { width, height } = a;
  const ya = luma(a);
  const yb = luma(b);
  const C1 = (0.01 * 255) ** 2;
  const C2 = (0.03 * 255) ** 2;
  const WIN = 8;
  const STEP = 4;
  const n = WIN * WIN;

  let total = 0;
  let count = 0;
  for (let y = 0; y + WIN <= height; y += STEP) {
    for (let x = 0; x + WIN <= width; x += STEP) {
      let sa = 0, sb = 0, saa = 0, sbb = 0, sab = 0;
      for (let dy = 0; dy < WIN; dy++) {
        let i = (y + dy) * width + x;
        for (let dx = 0; dx < WIN; dx++, i++) {
          const va = ya[i];
          const vb = yb[i];
          sa += va;
          sb += vb;
          saa += va * va;
          sbb += vb * vb;
          sab += va * vb;
        }
      }
      const ma = sa / n;
      const mb = sb / n;
      const va = saa / n - ma * ma;
      const vb = sbb / n - mb * mb;
      const cov = sab / n - ma * mb;
      total += ((2 * ma * mb + C1) * (2 * cov + C2)) / ((ma * ma + mb * mb + C1) * (va + vb + C2));
      count++;
    }
  }
  return count ? total / count : 1;
}

export function psnr(a: ImageData, b: ImageData): number {
  let se = 0;
  let n = 0;
  for (let p = 0; p < a.data.length; p += 4) {
    for (let c = 0; c < 3; c++) {
      const d = a.data[p + c] - b.data[p + c];
      se += d * d;
      n++;
    }
  }
  const mse = se / n;
  return mse === 0 ? Infinity : 10 * Math.log10((255 * 255) / mse);
}

/** 인코딩된 이미지(Blob)를 브라우저로 디코드해서 픽셀을 얻는다. 두 방식 모두 같은 디코더를 거치므로 공정하다. */
export async function decodeToImageData(blob: Blob): Promise<ImageData> {
  const bmp = await createImageBitmap(blob, { imageOrientation: "from-image" });
  const canvas = new OffscreenCanvas(bmp.width, bmp.height);
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(bmp, 0, 0);
  bmp.close();
  return ctx.getImageData(0, 0, canvas.width, canvas.height);
}
