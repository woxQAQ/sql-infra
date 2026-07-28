/// <reference lib="webworker" />

import type { WorkerRequest, WorkerResponse } from "./types";
import { loadWasmCompletion } from "./wasm";

const completion = loadWasmCompletion();

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const startedAt = performance.now();
  let response: WorkerResponse["response"];
  try {
    response = (await completion).complete({
      source: event.data.source,
      cursorUtf16: event.data.cursorUtf16,
      catalog: event.data.catalog,
    });
  } catch (error) {
    response = {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
  const message: WorkerResponse = {
    id: event.data.id,
    elapsedMs: performance.now() - startedAt,
    response,
  };
  self.postMessage(message);
};

export {};
