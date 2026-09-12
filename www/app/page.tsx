import Converter from "@/components/Converter";
import styles from "./page.module.css";

export default function Home() {
  return (
    <main className={styles.main}>
      <header className={styles.header}>
        <h1>pixelvault</h1>
        <p>
          이미지를 서버에 올리지 않고 <strong>브라우저 안에서</strong> 리사이즈 · 포맷 변환합니다.
          <br />
          Rust → WebAssembly. 파일은 이 탭 밖으로 나가지 않습니다.
        </p>
      </header>
      <Converter />
    </main>
  );
}
