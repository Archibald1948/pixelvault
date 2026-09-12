import type { Metadata } from "next";
import Benchmark from "@/components/Benchmark";
import styles from "../page.module.css";

export const metadata: Metadata = {
  title: "벤치마크 — pixelvault",
};

export default function BenchPage() {
  return (
    <main className={styles.main}>
      <header className={styles.header}>
        <h1>Canvas API vs WASM</h1>
        <p>
          같은 이미지를 <strong>브라우저 Canvas API(순수 JS)</strong> 와 <strong>pixelvault(Rust → WASM, Web Worker)</strong>{" "}
          로 처리해서 시간 · 용량 · 품질 · 메인 스레드 점유를 비교합니다.
        </p>
      </header>
      <Benchmark />
      <section className={styles.notes}>
        <h2>측정 방법</h2>
        <ul>
          <li>
            <strong>Canvas API</strong>: <code>createImageBitmap</code> → <code>OffscreenCanvas.drawImage</code>(
            <code>imageSmoothingQuality: &quot;high&quot;</code>) → <code>convertToBlob</code>. 메인 스레드에서 실행.
          </li>
          <li>
            <strong>WASM</strong>: <code>createPixelVault().process(file)</code>. 파일 읽기·디코드·리사이즈(Lanczos3)·인코드가
            전부 Web Worker 에서. 스레드 간 전송 시간 포함.
          </li>
          <li>각 조합마다 워밍업 1회를 버리고 N회 측정한 중앙값.</li>
          <li>
            <strong>SSIM</strong>: 각 방식의 <em>무손실(PNG) 리사이즈 결과</em> 대비 — 인코딩으로 잃은 품질. (한쪽 리사이즈를
            기준으로 삼으면 그쪽에 유리해지므로.) 리사이즈 필터 자체의 차이는 아래 확대 비교로 눈으로 확인하세요.
          </li>
          <li>
            <strong>메인 스레드 최대 멈춤</strong>: 처리 1회 동안 <code>MessageChannel</code> 이벤트 사이의 최대 간격.
          </li>
        </ul>
        <h2>기능 비교</h2>
        <div style={{ overflowX: "auto" }}>
          <table className={styles.featureTable}>
            <thead>
              <tr>
                <th />
                <th>Canvas API</th>
                <th>pixelvault (WASM)</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>EXIF / GPS</td>
                <td>항상 전부 삭제 (선택 불가)</td>
                <td>기본 삭제, 유지 선택 가능 (방향 태그 자동 초기화)</td>
              </tr>
              <tr>
                <td>ICC 색 프로파일 (Display P3 등)</td>
                <td>sRGB 로 변환 후 삭제</td>
                <td>그대로 보존</td>
              </tr>
              <tr>
                <td>WebP 인코딩</td>
                <td>Safari 미지원 (PNG 로 대체)</td>
                <td>모든 브라우저 (libwebp 내장)</td>
              </tr>
              <tr>
                <td>리사이즈 필터</td>
                <td>브라우저마다 다름 (보통 bilinear 계열)</td>
                <td>Lanczos3, 모든 브라우저에서 동일한 결과</td>
              </tr>
              <tr>
                <td>BlurHash</td>
                <td>별도 라이브러리 필요</td>
                <td>내장</td>
              </tr>
              <tr>
                <td>추가 다운로드</td>
                <td>0</td>
                <td>wasm ≈ 400 KB (gzip)</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </main>
  );
}
