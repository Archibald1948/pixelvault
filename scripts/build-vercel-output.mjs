// Next.js 정적 내보내기(www/out)를 Vercel Build Output API v3 형식(www/.vercel/output)으로 감싼다.
//
//   PIXELVAULT_STATIC_EXPORT=1 npm --prefix www run build
//   node scripts/build-vercel-output.mjs
//   (cd www && npx vercel deploy --prebuilt --prod)
//
// 왜 이렇게 하나: 이 데모는 서버 기능이 하나도 없는 정적 사이트다. Vercel 의 Next.js 빌더를 쓰면
// 레포 루트/프로젝트 루트 설정에 얽히는데(모노레포라 www 가 하위 폴더), 정적 파일만 올리면 그런 게 없다.
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "www/out");
const target = join(root, "www/.vercel/output");

if (!existsSync(out)) {
  console.error("www/out 이 없습니다. 먼저: PIXELVAULT_STATIC_EXPORT=1 npm --prefix www run build");
  process.exit(1);
}

rmSync(target, { recursive: true, force: true });
mkdirSync(join(target, "static"), { recursive: true });
cpSync(out, join(target, "static"), { recursive: true });

// 정적 내보내기는 /bench 를 bench.html 로 만든다. 확장자 없는 경로로 서빙되도록 매핑해 준다.
const overrides = {};
const walk = (dir) => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(full);
    } else if (entry.name.endsWith(".html")) {
      const file = relative(out, full).split("\\").join("/");
      const path = file.replace(/\.html$/, "").replace(/(^|\/)index$/, "");
      if (path) overrides[file] = { path, contentType: "text/html; charset=utf-8" };
    }
  }
};
walk(out);

writeFileSync(
  join(target, "config.json"),
  JSON.stringify(
    {
      version: 3,
      overrides,
      // 없는 경로는 정적 404 페이지로
      routes: [{ handle: "error" }, { status: 404, src: "/.*", dest: "/404.html" }],
    },
    null,
    2,
  ) + "\n",
);

console.log(`✅ ${relative(root, target)} 준비 완료`);
console.log(`   HTML 경로 매핑 ${Object.keys(overrides).length}개:`, Object.values(overrides).map((o) => "/" + o.path).join(", "));
