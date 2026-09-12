// 워커 풀: 워커 N 개를 띄워 두고, 들어오는 작업을 놀고 있는 워커에게 나눠 준다.
//
// 워커마다 wasm 인스턴스(와 메모리)가 따로 있다. 메모리를 공유하지 않으므로 SharedArrayBuffer 도,
// 그걸 위한 COOP/COEP 헤더도 필요 없다. 대신 워커 수만큼 메모리를 쓰므로 상한을 둔다.
import * as Comlink from "comlink";
import type { WorkerApi } from "./worker.js";

type Remote = Comlink.Remote<WorkerApi>;

interface Slot {
  worker: Worker;
  api: Remote;
}

interface Job {
  run: (api: Remote) => Promise<unknown>;
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
}

export class WorkerPool {
  readonly size: number;
  #slots: Slot[] = [];
  #idle: Slot[] = [];
  #queue: Job[] = [];
  #terminated = false;

  constructor(size: number) {
    this.size = Math.max(1, Math.floor(size));
  }

  /** 놀고 있는 워커에게 `job` 을 맡긴다. 모두 바쁘면 큐에서 기다린다. */
  run<T>(job: (api: Remote) => Promise<T>): Promise<T> {
    if (this.#terminated) return Promise.reject(new Error("PixelVault has been terminated"));
    return new Promise<T>((resolve, reject) => {
      this.#queue.push({ run: job, resolve: resolve as (v: unknown) => void, reject });
      this.#pump();
    });
  }

  /** 워커를 미리 전부 띄우고 wasm 을 로드해 둔다 (첫 작업 지연 줄이기). */
  async warmup(): Promise<void> {
    while (this.#slots.length < this.size) this.#idle.push(this.#spawn());
    await Promise.all(this.#slots.map((s) => s.api.warmup()));
  }

  terminate(): void {
    this.#terminated = true;
    for (const s of this.#slots) {
      s.api[Comlink.releaseProxy]();
      s.worker.terminate();
    }
    for (const job of this.#queue) job.reject(new Error("PixelVault has been terminated"));
    this.#slots = [];
    this.#idle = [];
    this.#queue = [];
  }

  #pump(): void {
    while (this.#queue.length > 0) {
      const slot = this.#idle.pop() ?? (this.#slots.length < this.size ? this.#spawn() : undefined);
      if (!slot) return; // 전부 바쁨 → 누가 끝나면 다시 pump
      const job = this.#queue.shift()!;
      job
        .run(slot.api)
        .then(job.resolve, job.reject)
        .finally(() => {
          if (this.#terminated) return;
          this.#idle.push(slot);
          this.#pump();
        });
    }
  }

  #spawn(): Slot {
    // 이 `new Worker(new URL("./worker.js", import.meta.url))` 모양을 webpack/Vite/Turbopack 이 인식해서
    // worker.js 와 그 의존성(wasm 포함)을 별도 번들로 만들어 준다. 모양을 바꾸면 번들러가 못 알아본다.
    const worker = new Worker(new URL("./worker.js", import.meta.url), {
      type: "module",
      name: `pixelvault-${this.#slots.length}`,
    });
    const slot = { worker, api: Comlink.wrap<WorkerApi>(worker) };
    this.#slots.push(slot);
    return slot;
  }
}
