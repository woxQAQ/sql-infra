import { onScopeDispose, ref, shallowRef } from "vue";

import { DEFAULT_CATALOG, INITIAL_QUERY } from "../data";
import { monaco, type MonacoEditor, type MonacoModel } from "../monaco";
import type {
  CatalogDocument,
  CompletionItemDto,
  CompletionResponseDto,
} from "../types";
import { CompletionWorkerClient, type TimedCompletion } from "../worker-client";

export type AnalysisState = "loading" | "running" | "ready" | "warning" | "error";

export interface AnalysisStatus {
  state: AnalysisState;
  label: string;
  detail?: string;
}

export function usePlayground() {
  const client = new CompletionWorkerClient();
  const sqlEditor = shallowRef<MonacoEditor>();
  const sqlModel = shallowRef<MonacoModel>();
  const catalogModel = shallowRef<MonacoModel>();
  const result = shallowRef<CompletionResponseDto>();
  const timing = ref("Waiting for first result");
  const sqlMeta = ref("Starting WASM worker");
  const catalogValid = ref(true);
  const catalogMessage = ref("Valid JSON");
  const status = ref<AnalysisStatus>({ state: "loading", label: "Starting" });

  let lastCatalog = JSON.parse(DEFAULT_CATALOG) as CatalogDocument;
  let generation = 0;
  let analysisTimer: number | undefined;
  let provider: monaco.IDisposable | undefined;
  let started = false;

  function attachSql(editor: MonacoEditor, model: MonacoModel): void {
    sqlEditor.value = editor;
    sqlModel.value = model;
    maybeStart();
  }

  function attachCatalog(_editor: MonacoEditor, model: MonacoModel): void {
    catalogModel.value = model;
    maybeStart();
  }

  function maybeStart(): void {
    if (started || !sqlEditor.value || !sqlModel.value || !catalogModel.value) return;
    started = true;
    registerCompletionProvider();
    loadInitialQuery();
  }

  function parseCatalog(reportError = true): CatalogDocument | undefined {
    const model = catalogModel.value;
    if (!model) return undefined;
    try {
      const parsed = JSON.parse(model.getValue()) as unknown;
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("Catalog root must be a JSON object");
      }
      lastCatalog = parsed as CatalogDocument;
      catalogValid.value = true;
      catalogMessage.value = "Valid JSON";
      return lastCatalog;
    } catch (error) {
      if (reportError) {
        const message = error instanceof Error ? error.message : String(error);
        catalogValid.value = false;
        catalogMessage.value = message;
        status.value = {
          state: "error",
          label: "Catalog error",
          detail: "Fix Catalog JSON to resume completion.",
        };
      }
      return undefined;
    }
  }

  function scheduleAnalysis(delay = 90): void {
    generation += 1;
    const requestedGeneration = generation;
    if (analysisTimer !== undefined) window.clearTimeout(analysisTimer);
    analysisTimer = window.setTimeout(
      () => void refreshAnalysis(requestedGeneration),
      delay,
    );
  }

  async function refreshAnalysis(requestedGeneration: number): Promise<void> {
    const editor = sqlEditor.value;
    const model = sqlModel.value;
    const catalog = parseCatalog();
    if (!editor || !model || !catalog) return;
    const position = editor.getPosition() ?? { lineNumber: 1, column: 1 };
    status.value = { state: "running", label: "Collecting" };
    try {
      const completion = await client.complete(
        model.getValue(),
        model.getOffsetAt(position),
        catalog,
      );
      if (requestedGeneration !== generation) return;
      applyResult(completion);
    } catch (error) {
      if (requestedGeneration !== generation) return;
      const message = error instanceof Error ? error.message : String(error);
      status.value = {
        state: "error",
        label: "Adapter error",
        detail: message,
      };
      catalogValid.value = false;
      catalogMessage.value = message;
    }
  }

  function applyResult(completion: TimedCompletion): void {
    result.value = completion.completion;
    const count = completion.completion.items.length;
    const duration = formatDuration(completion.elapsedMs);
    timing.value = `${duration} worker round-trip`;
    sqlMeta.value = `${count} candidate${count === 1 ? "" : "s"}, ${duration}`;
    const recoveryCount = completion.completion.context.recovery.length;
    status.value = recoveryCount
      ? {
          state: "warning",
          label: "Recovered input",
          detail: `${recoveryCount} parser recovery issue${recoveryCount === 1 ? "" : "s"}`,
        }
      : { state: "ready", label: "Ready" };
    catalogValid.value = true;
    catalogMessage.value = "Valid JSON";
  }

  function registerCompletionProvider(): void {
    const model = sqlModel.value;
    if (!model) return;
    provider = monaco.languages.registerCompletionItemProvider("sql", {
      triggerCharacters: [".", '"', " "],
      async provideCompletionItems(activeModel, position, _context, cancellationToken) {
        if (activeModel !== model) return { suggestions: [] };
        const catalog = parseCatalog(false);
        if (!catalog) return { suggestions: [] };
        try {
          const completion = await client.complete(
            activeModel.getValue(),
            activeModel.getOffsetAt(position),
            catalog,
          );
          if (cancellationToken.isCancellationRequested) return { suggestions: [] };
          applyResult(completion);
          return {
            suggestions: completion.completion.items.map((item) =>
              toMonacoCompletion(activeModel, item),
            ),
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          status.value = {
            state: "error",
            label: "Completion failed",
            detail: message,
          };
          return { suggestions: [] };
        }
      },
    });
  }

  function loadInitialQuery(): void {
    const editor = sqlEditor.value;
    const model = sqlModel.value;
    if (!editor || !model) return;
    const marker = INITIAL_QUERY.indexOf("|");
    const source = INITIAL_QUERY.replace("|", "");
    model.setValue(source);
    const point = marker >= 0 ? marker : source.length;
    const position = model.getPositionAt(point);
    editor.setPosition(position);
    editor.revealPositionInCenterIfOutsideViewport(position);
    scheduleAnalysis(0);
  }

  function applyCandidate(item: CompletionItemDto): void {
    const editor = sqlEditor.value;
    const model = sqlModel.value;
    if (!editor || !model) return;
    const start = model.getPositionAt(item.replacementRange.start);
    const end = model.getPositionAt(item.replacementRange.end);
    editor.executeEdits("pg-completion-playground", [
      {
        range: new monaco.Range(
          start.lineNumber,
          start.column,
          end.lineNumber,
          end.column,
        ),
        text: item.insertText,
        forceMoveMarkers: true,
      },
    ]);
    editor.setPosition(
      model.getPositionAt(item.replacementRange.start + item.insertText.length),
    );
    editor.focus();
    if (item.triggerSuggest) {
      void editor.trigger("candidate", "editor.action.triggerSuggest", {});
    }
  }

  function onSqlChange(): void {
    scheduleAnalysis();
  }

  function onSqlCursor(): void {
    scheduleAnalysis(70);
  }

  function onCatalogChange(): void {
    if (parseCatalog()) scheduleAnalysis(180);
  }

  onScopeDispose(() => {
    if (analysisTimer !== undefined) window.clearTimeout(analysisTimer);
    provider?.dispose();
    client.dispose();
  });

  return {
    result,
    status,
    timing,
    sqlMeta,
    catalogValid,
    catalogMessage,
    attachSql,
    attachCatalog,
    applyCandidate,
    onSqlChange,
    onSqlCursor,
    onCatalogChange,
  };
}

function toMonacoCompletion(
  model: MonacoModel,
  item: CompletionItemDto,
): monaco.languages.CompletionItem {
  const start = model.getPositionAt(item.replacementRange.start);
  const end = model.getPositionAt(item.replacementRange.end);
  return {
    label: item.label,
    kind: mapMonacoKind(item),
    insertText: item.insertText,
    range: new monaco.Range(
      start.lineNumber,
      start.column,
      end.lineNumber,
      end.column,
    ),
    detail: item.detail,
    documentation: `Origin: ${item.origin}${item.objectKind ? ` / ${item.objectKind}` : ""}`,
    sortText: item.sortText,
    filterText: item.label,
    command: item.triggerSuggest
      ? { id: "editor.action.triggerSuggest", title: "Suggest members" }
      : undefined,
  };
}

function mapMonacoKind(item: CompletionItemDto): monaco.languages.CompletionItemKind {
  switch (item.kind) {
    case "column":
      return monaco.languages.CompletionItemKind.Field;
    case "function":
      return monaco.languages.CompletionItemKind.Function;
    case "table":
      return monaco.languages.CompletionItemKind.Struct;
    case "schema":
      return monaco.languages.CompletionItemKind.Module;
    case "type":
      return monaco.languages.CompletionItemKind.Class;
    case "user":
      return monaco.languages.CompletionItemKind.User;
    case "database":
      return monaco.languages.CompletionItemKind.Folder;
    case "reference":
      return monaco.languages.CompletionItemKind.Reference;
    case "keyword":
    case "phrase":
    case "privilege":
      return monaco.languages.CompletionItemKind.Keyword;
    default:
      return monaco.languages.CompletionItemKind.Value;
  }
}

function formatDuration(milliseconds: number): string {
  return milliseconds < 1
    ? `${Math.max(1, Math.round(milliseconds * 1000))} µs`
    : `${milliseconds.toFixed(milliseconds < 10 ? 1 : 0)} ms`;
}
