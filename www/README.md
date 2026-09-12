# pixelvault 데모 사이트

Next.js (App Router) 데모. 배포: **https://pixelvault-rouge.vercel.app** · 레포 루트의 [README](../README.md) 참고.

- `/` — 변환기: 여러 파일 → Web Worker 풀로 변환, 메인 스레드 모니터, EXIF/BlurHash 표시
- `/bench` — Canvas API vs WASM 벤치마크

```bash
# 먼저 레포 루트에서: ./scripts/build-wasm.sh && (cd packages/pixelvault && npm install && npm run build)
npm install
npm run dev
```

`@sc0031/pixelvault` 는 `file:../packages/pixelvault` 로 연결되어 있다. 패키지를 다시 빌드하면 바로 반영된다.
