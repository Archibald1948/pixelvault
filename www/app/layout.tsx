import type { Metadata } from "next";
import Link from "next/link";
import "./globals.css";

export const metadata: Metadata = {
  title: "pixelvault",
  description: "브라우저 안에서 끝나는 이미지 처리 파이프라인 — Rust → WebAssembly",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="ko">
      <body>
        <nav className="nav">
          <Link href="/" className="brand">
            pixelvault
          </Link>
          <Link href="/">변환기</Link>
          <Link href="/bench">벤치마크</Link>
          <a href="https://github.com/Archibald1948/pixelvault" target="_blank" rel="noreferrer">
            GitHub
          </a>
        </nav>
        {children}
      </body>
    </html>
  );
}
