# M5 — 패키징 & 배포

> 목표: 남이 `npm install` 해서 쓸 수 있는 패키지, README 의 벤치마크 표 + GIF, Vercel 데모.

## 이 마일스톤에서 준비된 것

| 항목 | 위치 | 상태 |
|---|---|---|
| npm 패키지 | `packages/pixelvault` | 빌드·`npm pack` 검증 완료. **배포는 직접** (아래) |
| 패키지 README (영문) | `packages/pixelvault/README.md` | ✅ |
| 서드파티 고지 | `packages/pixelvault/THIRD_PARTY_NOTICES.md` | ✅ libwebp·kamadak-exif(BSD), IJG |
| 루트 README + 벤치 표 + GIF | `README.md`, `docs/images/demo.gif` | ✅ |
| CI | `.github/workflows/ci.yml` | ✅ Linux 에서 통과 확인 |
| Vercel 배포 | `.github/workflows/deploy.yml` | ✅ 배포됨: https://pixelvault-rouge.vercel.app (Actions 자동 배포는 토큰 필요) |
| 번들러 호환성 | Next.js 16 (Turbopack), Vite 8 dev/build | ✅ 실제로 설치해서 확인 |

## npm 패키지 구조

```
@archibald1948/pixelvault
├── dist/
│   ├── index.js / .d.ts     공개 API (processImage, createPixelVault, decodeBlurhash)
│   ├── worker.js            Web Worker 진입점
│   ├── pool.js, core.js     워커 풀, 공용 로직
│   └── wasm/
│       ├── pixelvault.js    wasm-bindgen 이 만든 glue
│       └── pixelvault_bg.wasm
├── README.md, LICENSE, THIRD_PARTY_NOTICES.md
```

빌드 순서 (`npm run build`):
1. `scripts/copy-wasm.mjs src` — `/pkg` 의 wasm 산출물을 `src/wasm` 으로 복사 (TS 가 `.d.ts` 로 타입을 알 수 있게)
2. `tsc` — `src/*.ts` → `dist/*.js` + `.d.ts` (번들링 없이 파일 그대로. `new URL(..., import.meta.url)` 모양을 보존해야 사용자의 번들러가 워커·wasm 을 찾는다)
3. `scripts/copy-wasm.mjs dist` — wasm 산출물을 `dist/wasm` 으로

> **왜 번들링(esbuild/tsup)하지 않나?** 번들러가 `new Worker(new URL("./worker.js", import.meta.url))` 를 자기 방식으로 바꿔 버리면
> 이 패키지를 쓰는 사람의 번들러가 워커를 인식하지 못한다. 라이브러리는 "ES 모듈 파일 그대로" 배포하고, 번들링은 앱이 하는 게 맞다.

### 버전 관리

`package.json` 의 `version` 을 [SemVer](https://semver.org/lang/ko/) 로 올린다. 0.x 동안은 마이너 버전이 깨지는 변경을 뜻해도 괜찮다는 게 관례.

## npm 배포 (직접 해야 하는 부분)

> ⚠️ 스코프 `@archibald1948` 는 **npm 사용자명과 같아야** 한다. npm 계정 이름이 다르면 `package.json` 의 `name` 과
> `www/package.json` 의 의존성 이름, `www` 코드의 import 경로를 바꾸자. (`pixelvault` 라는 스코프 없는 이름은 이미 누가 쓰고 있다)

```bash
npm login                          # 한 번만
./scripts/build-wasm.sh            # 최신 wasm
cd packages/pixelvault
npm run build
npm pack --dry-run                 # 들어갈 파일 목록 최종 확인 (dist/, README, LICENSE, NOTICES)
npm publish                        # publishConfig.access=public 이라 스코프 패키지도 공개로 올라감
```

`prepublishOnly` 스크립트가 `npm run build` 를 한 번 더 돌리므로 빌드를 잊어도 안전하다. (단 `/pkg` 는 미리 만들어져 있어야 한다)

## Vercel 배포

> 배포됨: **https://pixelvault-rouge.vercel.app**

### 왜 Vercel 이 직접 빌드하게 하지 않나

Vercel 의 기본 빌드 서버에는 Rust, wasm-pack, 그리고 **wasm32 를 지원하는 clang**(libwebp 컴파일용)이 없다.
그래서 **로컬 또는 GitHub Actions 에서 전부 빌드하고, 결과물만 Vercel 로 올리는** prebuilt 방식을 쓴다.

### 왜 정적 사이트로 내보내나

이 데모는 **서버에서 하는 일이 하나도 없다**(모든 페이지가 정적, 이미지 처리는 브라우저에서).
그래서 Next.js 의 정적 내보내기(`output: "export"`)로 HTML/JS/wasm 파일만 만들어 올린다.

- Vercel 의 Next.js 빌더를 쓰면 모노레포 구조(`www` 가 하위 폴더)와 프로젝트 Root Directory 설정에 얽힌다.
  실제로 `vercel build` 가 `www/www/.next` 를 찾는 문제를 만났다.
- 정적 파일만 올리면 그런 설정이 필요 없고, 빌드도 우리가 이미 하고 있다.
- `next start`(벤치마크용 로컬 서버)는 정적 내보내기 모드와 함께 쓸 수 없으므로,
  `next.config.ts` 에서 **`PIXELVAULT_STATIC_EXPORT=1` 일 때만** 내보내기 모드가 되게 했다.

```bash
./scripts/build-wasm.sh
(cd packages/pixelvault && npm run build)
PIXELVAULT_STATIC_EXPORT=1 npm --prefix www run build   # → www/out
node scripts/build-vercel-output.mjs                     # → www/.vercel/output (Build Output API v3)
(cd www && npx vercel deploy --prebuilt --prod)
```

`scripts/build-vercel-output.mjs` 가 하는 일은 두 가지다:
1. `www/out` 을 `.vercel/output/static` 으로 복사
2. `config.json` 에 **경로 매핑**을 적는다 — 정적 내보내기는 `/bench` 를 `bench.html` 로 만들기 때문에,
   이걸 안 하면 `/bench` 가 404 가 된다(실제로 첫 배포에서 겪었다). 404 페이지 라우트도 여기서 연결한다.

### 처음 한 번: 프로젝트 연결

```bash
cd www
npx vercel link --yes --project pixelvault   # .vercel/project.json 생성 (gitignore 됨)
```

### GitHub Actions 로 자동 배포하기

1. Vercel 대시보드 → Account Settings → Tokens 에서 토큰 발급
2. GitHub 레포 → Settings → Secrets and variables → Actions
   - Secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID` (뒤의 둘은 `www/.vercel/project.json` 에 있다)
   - Variables: `VERCEL_DEPLOY` = `true`
3. main 에 push 하거나 Actions 탭에서 "Deploy demo (Vercel)" 를 수동 실행

## CI (`.github/workflows/ci.yml`)

| 잡 | 하는 일 |
|---|---|
| `rust` | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` |
| `web` | wasm32 clippy → `build-wasm.sh`(링크 검증 + **크기 예산 500KB**) → `test-wasm.sh` → 패키지 빌드 + `npm pack --dry-run` → www lint + build |

Definition of Done 의 자동 검증 가능한 항목(테스트, clippy, wasm 크기)이 전부 CI 에 걸려 있다. 크기가 예산을 넘으면 빌드가 실패한다.

## README 에셋 다시 만들기

```bash
cd www && npm run build && npm start           # 터미널 1 (포트 3000)
node bench/browser-bench.mjs                   # 벤치 표 → bench/results/<날짜>-headless-chrome.md
node scripts/record-demo.mjs                   # 데모 GIF → docs/images/demo.gif (ffmpeg 필요)
```

둘 다 **헤드리스 Chrome 을 임시 프로필로** 띄워서 돌린다. 평소 쓰는 브라우저에는 영향이 없고, 창이 가려져서 느려지는 문제도 없다.
GIF 는 DevTools Protocol 의 `Page.captureScreenshot` 으로 프레임을 찍고, ffmpeg 2-pass(팔레트 생성 → 적용)로 만든다.

## 배포 전 체크리스트

- [ ] `cargo test && cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `./scripts/build-wasm.sh` (크기 예산 통과)
- [ ] `./scripts/test-wasm.sh`
- [ ] `packages/pixelvault/package.json` 의 `version` 올리기
- [ ] `npm pack --dry-run` 으로 파일 목록 확인
- [ ] 벤치마크 수치가 바뀌었으면 README 표 갱신
