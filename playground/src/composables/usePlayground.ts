import { onScopeDispose, ref, shallowRef } from "vue";

import { DEFAULT_CATALOG_DOCUMENT, INITIAL_QUERY } from "../data";
import { monaco, type MonacoEditor, type MonacoModel } from "../monaco";
import type {
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
  const result = shallowRef<CompletionResponseDto>();
  const status = ref<AnalysisStatus>({ state: "loading", label: "Starting" });
  const catalog = DEFAULT_CATALOG_DOCUMENT;

  let generation = 0;
  let analysisTimer: number | undefined;
  let provider: monaco.IDisposable | undefined;
  let started = false;

  function attachSql(editor: MonacoEditor, model: MonacoModel): void {
    sqlEditor.value = editor;
    sqlModel.value = model;
    maybeStart();
  }

  function maybeStart(): void {
    if (started || !sqlEditor.value || !sqlModel.value) return;
    started = true;
    registerCompletionProvider();
    loadInitialQuery();
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
    if (!editor || !model) return;
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
    }
  }

  function applyResult(completion: TimedCompletion): void {
    result.value = completion.completion;
    const diagnosticCount = completion.completion.context.diagnostics.length;
    status.value = diagnosticCount
      ? {
          state: "warning",
          label: "Completion diagnostics",
          detail: `${diagnosticCount} completion diagnostic${diagnosticCount === 1 ? "" : "s"}`,
        }
      : { state: "ready", label: "Ready" };
  }

  function registerCompletionProvider(): void {
    const model = sqlModel.value;
    if (!model) return;
    provider = monaco.languages.registerCompletionItemProvider("sql", {
      triggerCharacters: [".", '"', " "],
      async provideCompletionItems(activeModel, position, _context, cancellationToken) {
        if (activeModel !== model) return { suggestions: [] };
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

  onScopeDispose(() => {
    if (analysisTimer !== undefined) window.clearTimeout(analysisTimer);
    provider?.dispose();
    client.dispose();
  });

  return {
    result,
    status,
    attachSql,
    applyCandidate,
    onSqlChange,
    onSqlCursor,
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
