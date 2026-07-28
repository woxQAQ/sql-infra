import wasmUrl from "./generated/pg_completion_playground.wasm?url";
import type { CatalogDocument, WireResponse } from "./types";

interface PlaygroundExports extends WebAssembly.Exports {
  memory: WebAssembly.Memory;
  playground_alloc(length: number): number;
  playground_complete(pointer: number, length: number): number;
  playground_result_len(): number;
}

export interface WasmCompletionRequest {
  source: string;
  cursorUtf16: number;
  catalog: CatalogDocument;
}

export interface WasmCompletion {
  complete(request: WasmCompletionRequest): WireResponse;
}

async function instantiate(): Promise<WebAssembly.Instance> {
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error(`Unable to load completion WASM (${response.status})`);
  }
  if (WebAssembly.instantiateStreaming) {
    try {
      return (await WebAssembly.instantiateStreaming(response.clone(), {})).instance;
    } catch {
      // Some static servers do not send application/wasm. The byte fallback
      // keeps the playground portable without hiding a real fetch failure.
    }
  }
  return (await WebAssembly.instantiate(await response.arrayBuffer(), {})).instance;
}

export async function loadWasmCompletion(): Promise<WasmCompletion> {
  const instance = await instantiate();
  const exports = instance.exports as PlaygroundExports;
  if (
    !(exports.memory instanceof WebAssembly.Memory) ||
    typeof exports.playground_alloc !== "function" ||
    typeof exports.playground_complete !== "function" ||
    typeof exports.playground_result_len !== "function"
  ) {
    throw new Error("Completion WASM exports do not match the playground interface");
  }

  const encoder = new TextEncoder();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  return {
    complete(request) {
      const input = encoder.encode(JSON.stringify(request));
      const requestPointer = exports.playground_alloc(input.byteLength);
      new Uint8Array(exports.memory.buffer, requestPointer, input.byteLength).set(input);
      const responsePointer = exports.playground_complete(
        requestPointer,
        input.byteLength,
      );
      const responseLength = exports.playground_result_len();
      // `complete` may grow WASM memory, so obtain the buffer only after it
      // returns and copy before the single response slot is reused.
      const output = new Uint8Array(
        exports.memory.buffer,
        responsePointer,
        responseLength,
      ).slice();
      return JSON.parse(decoder.decode(output)) as WireResponse;
    },
  };
}
