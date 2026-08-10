import CompletionWorker from "./completion.worker?worker";
import type {
  CatalogDocument,
  CompletionResponseDto,
  WorkerRequest,
  WorkerResponse,
} from "./types";

interface PendingRequest {
  resolve(value: TimedCompletion): void;
  reject(reason: Error): void;
}

export interface TimedCompletion {
  completion: CompletionResponseDto;
  elapsedMs: number;
}

export class CompletionWorkerClient {
  readonly #worker = new CompletionWorker();
  readonly #pending = new Map<number, PendingRequest>();
  #nextId = 1;

  constructor() {
    this.#worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const pending = this.#pending.get(event.data.id);
      if (!pending) return;
      this.#pending.delete(event.data.id);
      if (!event.data.response.ok || !event.data.response.completion) {
        pending.reject(
          new Error(event.data.response.error ?? "Completion worker returned no result"),
        );
        return;
      }
      pending.resolve({
        completion: event.data.response.completion,
        elapsedMs: event.data.elapsedMs,
      });
    };
    this.#worker.onerror = (event) => {
      const error = new Error(event.message || "Completion worker crashed");
      for (const pending of this.#pending.values()) pending.reject(error);
      this.#pending.clear();
    };
  }

  complete(
    source: string,
    cursorUtf16: number,
    catalog: CatalogDocument,
  ): Promise<TimedCompletion> {
    const id = this.#nextId++;
    const request: WorkerRequest = { id, source, cursorUtf16, catalog };
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#worker.postMessage(request);
    });
  }

  dispose(): void {
    this.#worker.terminate();
    const error = new Error("Completion worker disposed");
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
  }
}
