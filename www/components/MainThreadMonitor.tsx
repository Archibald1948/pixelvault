"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import styles from "./MainThreadMonitor.module.css";

/**
 * 메인 스레드가 막히는지 눈으로 보여 주는 모니터.
 *
 * - 공이 부드럽게 움직이면 메인 스레드가 자유롭다. 멈추면 막힌 것.
 * - "최장 프레임 간격": requestAnimationFrame 콜백 사이 최대 시간. 60fps 면 ~16ms, 막히면 수백~수천 ms.
 * - "Long Task": 브라우저가 50ms 넘게 메인 스레드를 점유한 작업을 알려 준다(Chrome 계열).
 *
 * `measuring` 이 true 가 되는 순간 통계를 초기화하고, 그 동안의 최댓값을 기록한다.
 */
export default function MainThreadMonitor({ measuring }: { measuring: boolean }) {
  const [fps, setFps] = useState(0);
  const [maxGap, setMaxGap] = useState(0);
  const [longest, setLongest] = useState(0);
  const longTaskSupported = useSyncExternalStore(
    noopSubscribe,
    () => PerformanceObserver.supportedEntryTypes?.includes("longtask") ?? false,
    () => true, // 서버 렌더링 때는 일단 지원한다고 가정
  );
  const ballRef = useRef<HTMLDivElement>(null);
  const stats = useRef({ maxGap: 0, longest: 0 });
  const measuringRef = useRef(measuring);

  // 측정이 "시작되는 순간" 통계를 초기화한다 (다음 프레임에서 rAF 루프가 처리)
  useEffect(() => {
    if (measuring && !measuringRef.current) stats.current = { maxGap: 0, longest: 0 };
    measuringRef.current = measuring;
  }, [measuring]);

  // rAF 루프: 공 애니메이션 + FPS + 프레임 간격
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let frames = 0;
    let windowStart = last;
    const tick = (now: number) => {
      const gap = now - last;
      last = now;
      frames++;
      if (gap > stats.current.maxGap) stats.current.maxGap = gap;
      if (now - windowStart >= 500) {
        setFps(Math.round((frames * 1000) / (now - windowStart)));
        setMaxGap(stats.current.maxGap);
        setLongest(stats.current.longest);
        frames = 0;
        windowStart = now;
      }
      if (ballRef.current) {
        const x = (Math.sin(now / 400) + 1) / 2;
        ballRef.current.style.transform = `translateX(${x * 100}%)`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  // Long Task 관찰
  useEffect(() => {
    if (!longTaskSupported) return;
    const obs = new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        if (e.duration > stats.current.longest) stats.current.longest = e.duration;
      }
    });
    obs.observe({ type: "longtask" });
    return () => obs.disconnect();
  }, [longTaskSupported]);

  const blocked = maxGap > 200;

  return (
    <div className={styles.monitor} data-testid="monitor">
      <div className={styles.track}>
        <div ref={ballRef} className={styles.ballWrap}>
          <div className={`${styles.ball} ${blocked ? styles.bad : ""}`} />
        </div>
      </div>
      <div className={styles.numbers}>
        <span>
          메인 스레드 <strong>{fps}</strong> fps
        </span>
        <span>
          최장 프레임 간격 <strong data-testid="max-gap">{maxGap.toFixed(0)}</strong> ms
        </span>
        <span>
          최장 Long Task{" "}
          <strong data-testid="longest-task">
            {longTaskSupported ? `${longest.toFixed(0)} ms` : "미지원"}
          </strong>
        </span>
      </div>
    </div>
  );
}

function noopSubscribe() {
  return () => {};
}
