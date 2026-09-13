# M5 — 패키징 & 배포

> 목표: 남이 `npm install` 해서 쓸 수 있는 패키지, README 의 벤치마크 표 + GIF, Vercel 데모.

## 이 마일스톤에서 준비된 것

| 항목 | 위치 | 상태 |
|---|---|---|
| npm 패키지 | `packages/pixelvault` | ✅ 배포됨: https://www.npmjs.com/package/@sc0031/pixelvault |
| 패키지 README (영문) | `packages/pixelvault/README.md` | ✅ |
| 서드파티 고지 | `packages/pixelvault/THIRD_PARTY_NOTICES.md` | ✅ libwebp·kamadak-exif(BSD), IJG |
| 루트 README + 벤치 표 + GIF | `README.md`, `docs/images/demo.gif` | ✅ |
| CI | `.github/workflows/ci.yml` | ✅ Linux 에서 통과 확인 |
| Vercel 배포 | `.github/workflows/deploy.yml` | ✅ 배포됨: https://pixelvault-rouge.vercel.app (Actions 자동 배포는 토큰 필요) |
| 번들러 호환성 | Next.js 16 (Turbopack), Vite 8 dev/build | ✅ 실제로 설치해서 확인 |

## npm 패키지 구조

```
@sc0031/pixelvault
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

## npm 배포

> 배포됨: **https://www.npmjs.com/package/@sc0031/pixelvault**
>
> ⚠️ 스코프는 **npm 사용자명과 같아야** 한다(여기서는 `sc0031`). 계정 이름이 다르면 `packages/pixelvault/package.json` 의
> `name`, `www/package.json` 의 의존성 이름, `www` 코드의 import 경로를 함께 바꿔야 한다.
> (`pixelvault` 라는 스코프 없는 이름은 이미 누가 쓰고 있다)

### 첫 배포에서 막혔던 것: 2FA

npm 은 2025~2026 년에 정책을 강화해서 **2FA 없이는 publish 자체가 거부**된다:

```
403 Forbidden - PUT https://registry.npmjs.org/@sc0031%2fpixelvault
Two-factor authentication or granular access token with bypass 2fa enabled is required to publish packages.
```

디버그 로그에서 원인을 특정한 단서는 **상태 코드**였다. npm CLI 는 OTP 를 물어볼 준비(`otplease`)까지 갔는데
레지스트리가 **401(OTP 요구)이 아니라 403** 을 돌려줬다 → "OTP 를 달라"가 아니라 "이 계정으로는 publish 불가"라는 뜻.
계정에 2FA 가 아예 없었던 것이다. (로그인할 때 받은 이메일 코드는 2FA 가 아니라 새 기기 확인용이다)

해결: npmjs.com → Account → Two-Factor Authentication 에서 **보안 키(패스키)** 를 등록.
그 뒤 `npm publish` 는 브라우저 승인 URL 을 띄우고, 패스키로 승인하면 배포가 진행된다.
npm CLI 도 **11.5.1 이상**이 필요하다(웹 기반 2FA 승인, trusted publishing). 10.x 는 URL 만 출력하고 끝난다.

```bash
npm login --auth-type=web
./scripts/build-wasm.sh
cd packages/pixelvault && npm run build
npm pack --dry-run    # 들어갈 파일 확인 (dist/, README, LICENSE, NOTICES)
npm publish           # publishConfig.access=public 이라 스코프 패키지도 공개로 올라간다
```

### 이후 버전: Trusted Publishing (토큰도 2FA 승인도 없이)

첫 배포 이후로는 **GitHub Actions 가 OIDC 로 자신을 증명**해서 배포한다. 레포에 비밀값을 저장하지 않고,
사람이 패스키를 누를 필요도 없다. 배포물에는 provenance(어느 커밋·워크플로에서 나왔는지)가 자동으로 붙는다.

**npm 쪽 설정** (패키지 → Settings → Trusted Publisher → GitHub Actions):

| 항목 | 값 |
|---|---|
| Organization or user | `Archibald1948` |
| Repository | `pixelvault` |
| Workflow filename | `publish.yml` (파일명만. `.github/workflows/` 안에 있어야 한다) |
| Allowed actions | ☑ Allow `npm publish` |

마지막 항목이 중요하다. 체크하지 않으면 `npm stage publish`(스테이징)만 허용되어, CI 가 올려도 사람이
npm 사이트에서 따로 공개해야 한다. "CI 는 올리기만, 공개는 내가 확인하고" 방식을 원하면 오히려 빼는 게 맞다.

**워크플로 쪽 요건** (`.github/workflows/publish.yml`):

```yaml
permissions:
  id-token: write                           # 없으면 OIDC 토큰이 발급되지 않는다
...
      - run: npm install --global npm@^11.9.0  # trusted publishing 은 npm 11.5.1+
      - run: npm publish                       # 토큰 설정 없음. OIDC 로 자동 인증
```

**릴리스 방법**

```bash
cd packages/pixelvault
npm version patch          # package.json 버전 상승 + 커밋 + v0.1.1 태그
git push --follow-tags     # 태그가 올라가면 워크플로가 빌드하고 배포한다
```

태그 이름과 `package.json` 의 버전이 다르면 배포 직전에 실패시킨다(사고 방지).
Actions 탭에서 수동 실행하면 기본이 dry-run 이라 빌드만 확인할 수 있다.

**검증 결과 (v0.1.1, 2026-09-13)** — 태그 `v0.1.1` 푸시 한 번으로 끝까지 자동으로 진행됐다:

```
✓ Check tag matches package version
✓ Publish
  npm notice publish Signed provenance statement with source and build information from GitHub Actions
  npm notice publish Provenance statement published to transparency log: https://search.sigstore.dev/?logIndex=2813646156
  + @sc0031/pixelvault@0.1.1
```

- 토큰·2FA 승인 없이 OIDC 로만 인증됨
- 레지스트리에 SLSA provenance(`https://slsa.dev/provenance/v1`) attestation 이 붙음
- 설치하는 쪽에서 검증 가능: `npm audit signatures` → *"1 package has a verified attestation"*
- npm 패키지 페이지에 "Built and signed on GitHub Actions" 표시가 생긴다 — 이 버전이 이 레포의 이 커밋에서 빌드됐다는 증명

> 참고: `npm version patch` 는 자동으로 커밋 메시지를 `0.1.1` 로만 만든다. 이 레포는 Conventional Commits 규칙을 쓰므로
> `npm version patch --no-git-tag-version` 으로 버전만 올리고, `chore(release): v0.1.1` 로 직접 커밋한 뒤 `git tag -a v0.1.1` 했다.

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

### Vercel 의 Git 자동 빌드는 꺼 둔다

`vercel link` 로 프로젝트를 만들면 GitHub 연동이 함께 켜져서, push 할 때마다 **Vercel 서버가 직접 빌드를 시도**한다.
이건 반드시 실패한다:

```
Error: No Next.js version detected. Make sure your package.json has "next" in either
"dependencies" or "devDependencies". Also check your Root Directory setting matches ...
```

- Next 앱은 레포 루트가 아니라 `www/` 에 있고,
- Root Directory 를 `www` 로 고쳐도 `npm ci` 단계에서 실패한다.
  `www` 는 `file:../packages/pixelvault` 에 의존하는데, 그 패키지의 `dist/` 는 **wasm 빌드가 선행되어야** 생기기 때문이다.
  그리고 Vercel 빌드 서버에는 Rust·wasm-pack·wasm32용 clang 이 없다.

그래서 레포 루트 `vercel.json` 으로 main 브랜치의 자동 배포를 끈다:

```json
{
  "git": { "deploymentEnabled": { "main": false } }
}
```

배포는 아래 prebuilt 방식(로컬 또는 GitHub Actions)으로만 한다. GitHub 커밋에 빨간 X 가 뜨지 않는다.

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
