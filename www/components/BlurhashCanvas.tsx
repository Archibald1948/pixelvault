"use client";

import { useEffect, useRef } from "react";
import { decodeBlurhash } from "@/lib/pixelvault";

/** BlurHash 를 작은 캔버스(32px)에 그리고 CSS 로 늘려서 보여준다. 흐린 이미지라 확대해도 티가 안 난다. */
export default function BlurhashCanvas({
  hash,
  aspect,
  className,
}: {
  hash: string;
  aspect: number;
  className?: string;
}) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const w = 32;
    const h = Math.max(1, Math.round(w / aspect));
    let cancelled = false;
    decodeBlurhash(hash, w, h).then((pixels) => {
      const canvas = ref.current;
      if (cancelled || !canvas) return;
      canvas.width = w;
      canvas.height = h;
      canvas.getContext("2d")?.putImageData(new ImageData(pixels, w, h), 0, 0);
    });
    return () => {
      cancelled = true;
    };
  }, [hash, aspect]);

  return <canvas ref={ref} className={className} style={{ aspectRatio: aspect }} />;
}
