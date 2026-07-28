import * as monaco from "monaco-editor/esm/vs/editor/editor.api.js";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker.js?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker.js?worker";
import "monaco-editor/esm/vs/language/json/monaco.contribution.js";
import "monaco-editor/esm/vs/basic-languages/sql/sql.contribution.js";
import "monaco-editor/esm/vs/editor/browser/coreCommands.js";
import "monaco-editor/esm/vs/editor/contrib/bracketMatching/browser/bracketMatching.js";
import "monaco-editor/esm/vs/editor/contrib/clipboard/browser/clipboard.js";
import "monaco-editor/esm/vs/editor/contrib/contextmenu/browser/contextmenu.js";
import "monaco-editor/esm/vs/editor/contrib/find/browser/findController.js";
import "monaco-editor/esm/vs/editor/contrib/folding/browser/folding.js";
import "monaco-editor/esm/vs/editor/contrib/hover/browser/hoverContribution.js";
import "monaco-editor/esm/vs/editor/contrib/linesOperations/browser/linesOperations.js";
import "monaco-editor/esm/vs/editor/contrib/suggest/browser/suggestController.js";
import "monaco-editor/esm/vs/editor/contrib/wordOperations/browser/wordOperations.js";

import "./style.css";
import type {
  CatalogDocument,
  CompletionItemDto,
  CompletionResponseDto,
  ContextDto,
  RelationDto,
} from "./types";
import { CompletionWorkerClient, type TimedCompletion } from "./worker-client";

self.MonacoEnvironment = {
  getWorker(_moduleId: string, label: string) {
    return label === "json" ? new JsonWorker() : new EditorWorker();
  },
};

const OBJECT_KINDS = [
  "Table",
  "View",
  "MaterializedView",
  "ForeignTable",
  "Sequence",
  "Index",
  "Column",
  "Attribute",
  "Function",
  "Procedure",
  "Routine",
  "Aggregate",
  "Type",
  "Domain",
  "Schema",
  "Constraint",
  "Collation",
  "Operator",
  "OperatorClass",
  "OperatorFamily",
  "Role",
  "Database",
  "AccessMethod",
  "Conversion",
  "EventTrigger",
  "Extension",
  "ForeignDataWrapper",
  "ForeignServer",
  "Language",
  "Policy",
  "PropertyGraph",
  "Publication",
  "Rule",
  "Statistics",
  "Subscription",
  "Tablespace",
  "TextSearchConfiguration",
  "TextSearchDictionary",
  "TextSearchParser",
  "TextSearchTemplate",
  "Trigger",
] as const;

const DEFAULT_CATALOG = JSON.stringify(
  {
    searchPath: ["public"],
    objects: [
      { kind: "Schema", name: ["public"] },
      { kind: "Schema", name: ["analytics"] },
      { kind: "Schema", name: ["u"] },
      {
        kind: "Table",
        name: ["public", "users"],
        detail: "application users",
        members: [
          { kind: "Column", name: "id", detail: "bigint · primary key" },
          { kind: "Column", name: "name", detail: "text" },
          { kind: "Column", name: "email", detail: "text" },
          { kind: "Column", name: "created_at", detail: "timestamptz" },
        ],
      },
      {
        kind: "Table",
        name: ["public", "orders"],
        detail: "customer orders",
        members: [
          { kind: "Column", name: "id", detail: "bigint · primary key" },
          { kind: "Column", name: "user_id", detail: "bigint" },
          { kind: "Column", name: "total", detail: "numeric(12, 2)" },
          { kind: "Column", name: "status", detail: "order_status" },
        ],
      },
      {
        kind: "Table",
        name: ["analytics", "events"],
        members: [
          { kind: "Column", name: "event_id", detail: "uuid" },
          { kind: "Column", name: "payload", detail: "jsonb" },
        ],
      },
      {
        kind: "Function",
        name: ["u", "refresh"],
        detail: "refresh() → void",
      },
      {
        kind: "Function",
        name: ["u", "rebuild_cache"],
        detail: "rebuild_cache(text) → boolean",
      },
      {
        kind: "Type",
        name: ["public", "order_status"],
        detail: "pending | paid | shipped",
      },
      { kind: "Sequence", name: ["public", "users_id_seq"] },
      { kind: "Role", name: ["app_reader"] },
    ],
  } satisfies CatalogDocument,
  null,
  2,
);

interface Example {
  name: string;
  description: string;
  source: string;
}

const EXAMPLES: Example[] = [
  {
    name: "Alias / schema collision",
    description: "u is both a relation alias and a schema",
    source: "SELECT u.|\nFROM public.users AS u;",
  },
  {
    name: "Correlated subquery",
    description: "inspect local and outer relation visibility",
    source:
      "SELECT o.id,\n       (SELECT u.|\n          FROM public.users AS u\n         WHERE u.id = o.user_id)\nFROM public.orders AS o;",
  },
  {
    name: "DDL container",
    description: "columns come from the CREATE INDEX target",
    source: "CREATE INDEX users_lookup ON public.users (|);",
  },
  {
    name: "DML pseudo-relation",
    description: "excluded maps back to the INSERT target",
    source:
      "INSERT INTO public.users (id, name)\nVALUES (1, 'Ada')\nON CONFLICT (id) DO UPDATE\nSET name = excluded.|;",
  },
  {
    name: "Explicit CTE columns",
    description: "syntax-known columns need no catalog lookup",
    source:
      "WITH active_users(user_id, display_name) AS (\n  SELECT id, name FROM public.users\n)\nSELECT a.|\nFROM active_users AS a;",
  },
  {
    name: "Qualified catalog object",
    description: "objects outside search_path insert a qualified name",
    source: "SELECT *\nFROM eve|;",
  },
];

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Missing #app mount point");

app.innerHTML = `
  <div class="shell">
    <header class="topbar">
      <div class="brand-block">
        <div class="brand-mark" aria-hidden="true"><span>pg</span><b>/</b></div>
        <div>
          <div class="brand-title">completion playground</div>
          <div class="brand-subtitle">parser-native context · caller-owned catalog</div>
        </div>
      </div>
      <div class="runtime-badge" title="Completion runs locally in a Web Worker">
        <span class="runtime-pulse"></span>
        Rust · WASM · local
      </div>
    </header>

    <div class="toolbar">
      <label class="example-picker">
        <span>Scenario</span>
        <select id="example-select" aria-label="Select a completion scenario">
          ${EXAMPLES.map((example, index) => `<option value="${index}">${escapeHtml(example.name)}</option>`).join("")}
        </select>
      </label>
      <span class="scenario-note" id="scenario-note"></span>
      <div class="toolbar-spacer"></div>
      <span class="cursor-position" id="cursor-position">Ln 1, Col 1</span>
      <button class="button secondary" id="reset-catalog" type="button">Reset catalog</button>
      <button class="button primary" id="trigger-completion" type="button">
        Complete <kbd>Ctrl</kbd><kbd>Space</kbd>
      </button>
    </div>

    <main class="workspace">
      <section class="editors-column" aria-label="Playground editors">
        <article class="editor-panel sql-panel">
          <div class="panel-header">
            <div>
              <span class="eyebrow">Query</span>
              <h2>PostgreSQL</h2>
            </div>
            <div class="panel-meta" id="sql-meta">waiting for WASM</div>
          </div>
          <div class="editor-host" id="sql-editor"></div>
        </article>

        <article class="editor-panel catalog-panel">
          <div class="panel-header">
            <div>
              <span class="eyebrow">Adapter input</span>
              <h2>Catalog JSON</h2>
            </div>
            <div class="catalog-state" id="catalog-state"><span></span>valid</div>
          </div>
          <div class="editor-host" id="catalog-editor"></div>
        </article>
      </section>

      <aside class="inspector-panel" aria-label="Completion inspector">
        <div class="inspector-header">
          <div>
            <span class="eyebrow">Live result</span>
            <h2>Completion context</h2>
          </div>
          <div class="analysis-status" id="analysis-status">
            <span class="status-dot"></span>
            <span id="analysis-status-text">starting</span>
          </div>
        </div>
        <nav class="tabs" aria-label="Inspector views">
          <button class="tab active" type="button" data-tab="candidates">Candidates <b id="candidate-count">0</b></button>
          <button class="tab" type="button" data-tab="intent">Intent</button>
          <button class="tab" type="button" data-tab="scope">Scope</button>
          <button class="tab" type="button" data-tab="raw">Raw</button>
        </nav>
        <div class="inspector-content" id="inspector-content">
          <div class="empty-state">
            <div class="empty-glyph">⌁</div>
            <p>Loading the completion engine…</p>
          </div>
        </div>
        <footer class="inspector-footer">
          <span id="timing">—</span>
          <span>All offsets shown in UTF‑16 and UTF‑8</span>
        </footer>
      </aside>
    </main>
  </div>
`;

function element<T extends Element>(selector: string): T {
  const value = document.querySelector<T>(selector);
  if (!value) throw new Error(`Missing element: ${selector}`);
  return value;
}

const sqlHost = element<HTMLDivElement>("#sql-editor");
const catalogHost = element<HTMLDivElement>("#catalog-editor");
const inspector = element<HTMLDivElement>("#inspector-content");
const status = element<HTMLDivElement>("#analysis-status");
const statusText = element<HTMLSpanElement>("#analysis-status-text");
const timing = element<HTMLSpanElement>("#timing");
const sqlMeta = element<HTMLDivElement>("#sql-meta");
const catalogState = element<HTMLDivElement>("#catalog-state");
const candidateCount = element<HTMLElement>("#candidate-count");
const exampleSelect = element<HTMLSelectElement>("#example-select");
const scenarioNote = element<HTMLSpanElement>("#scenario-note");
const cursorPosition = element<HTMLSpanElement>("#cursor-position");

monaco.editor.defineTheme("pg-studio", {
  base: "vs-dark",
  inherit: true,
  rules: [
    { token: "keyword.sql", foreground: "D6A85F", fontStyle: "bold" },
    { token: "string.sql", foreground: "9CC98C" },
    { token: "number.sql", foreground: "D78C76" },
    { token: "comment.sql", foreground: "68746C", fontStyle: "italic" },
    { token: "delimiter.sql", foreground: "89958D" },
  ],
  colors: {
    "editor.background": "#111411",
    "editor.foreground": "#DCE3DC",
    "editorLineNumber.foreground": "#465049",
    "editorLineNumber.activeForeground": "#91A097",
    "editorCursor.foreground": "#E6B866",
    "editor.selectionBackground": "#315A4A88",
    "editor.inactiveSelectionBackground": "#315A4A44",
    "editor.lineHighlightBackground": "#171C18",
    "editorSuggestWidget.background": "#171B18",
    "editorSuggestWidget.border": "#344038",
    "editorSuggestWidget.selectedBackground": "#294536",
    "editorSuggestWidget.highlightForeground": "#F2BF65",
    "editorWidget.background": "#171B18",
    "editorWidget.border": "#344038",
    "input.background": "#101310",
    "input.border": "#344038",
    "scrollbarSlider.background": "#4B5D5144",
    "scrollbarSlider.hoverBackground": "#60746866",
  },
});
monaco.editor.setTheme("pg-studio");

monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
  validate: true,
  allowComments: false,
  schemas: [
    {
      uri: "https://sql-infra.local/catalog.schema.json",
      fileMatch: ["inmemory://playground/catalog.json"],
      schema: {
        type: "object",
        additionalProperties: false,
        properties: {
          searchPath: { type: "array", items: { type: "string", minLength: 1 } },
          objects: {
            type: "array",
            items: {
              type: "object",
              additionalProperties: false,
              required: ["kind", "name"],
              properties: {
                kind: { enum: [...OBJECT_KINDS] },
                name: {
                  type: "array",
                  minItems: 1,
                  items: { type: "string", minLength: 1 },
                },
                detail: { type: "string" },
                members: {
                  type: "array",
                  items: {
                    type: "object",
                    additionalProperties: false,
                    required: ["kind", "name"],
                    properties: {
                      kind: { enum: [...OBJECT_KINDS] },
                      name: { type: "string", minLength: 1 },
                      detail: { type: "string" },
                    },
                  },
                },
              },
            },
          },
        },
      },
    },
  ],
});

const sqlModel = monaco.editor.createModel(
  "",
  "sql",
  monaco.Uri.parse("inmemory://playground/query.sql"),
);
const catalogModel = monaco.editor.createModel(
  DEFAULT_CATALOG,
  "json",
  monaco.Uri.parse("inmemory://playground/catalog.json"),
);

const sharedEditorOptions: monaco.editor.IStandaloneEditorConstructionOptions = {
  theme: "pg-studio",
  automaticLayout: true,
  fontFamily: '"Berkeley Mono", "SFMono-Regular", Consolas, monospace',
  fontSize: 13,
  lineHeight: 22,
  fontLigatures: true,
  minimap: { enabled: false },
  overviewRulerLanes: 0,
  hideCursorInOverviewRuler: true,
  renderLineHighlight: "all",
  scrollBeyondLastLine: false,
  smoothScrolling: true,
  padding: { top: 14, bottom: 14 },
  fixedOverflowWidgets: true,
  bracketPairColorization: { enabled: true },
};

const sqlEditor = monaco.editor.create(sqlHost, {
  ...sharedEditorOptions,
  model: sqlModel,
  wordWrap: "on",
  lineNumbersMinChars: 3,
  suggest: {
    showWords: false,
    showSnippets: false,
    preview: true,
    localityBonus: true,
  },
  quickSuggestions: { other: true, comments: false, strings: false },
  suggestOnTriggerCharacters: true,
});

const catalogEditor = monaco.editor.create(catalogHost, {
  ...sharedEditorOptions,
  model: catalogModel,
  fontSize: 12,
  lineHeight: 20,
  lineNumbers: "off",
  folding: true,
  wordWrap: "off",
  renderLineHighlight: "none",
});

const client = new CompletionWorkerClient();
let activeTab = "candidates";
let lastResult: CompletionResponseDto | undefined;
let lastCatalog: CatalogDocument = JSON.parse(DEFAULT_CATALOG) as CatalogDocument;
let analysisGeneration = 0;
let analysisTimer: number | undefined;

function parseCatalog(reportError = true): CatalogDocument | undefined {
  try {
    const parsed = JSON.parse(catalogModel.getValue()) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new Error("Catalog root must be a JSON object");
    }
    lastCatalog = parsed as CatalogDocument;
    catalogState.className = "catalog-state valid";
    catalogState.innerHTML = "<span></span>valid";
    return lastCatalog;
  } catch (error) {
    if (reportError) {
      const message = error instanceof Error ? error.message : String(error);
      catalogState.className = "catalog-state invalid";
      catalogState.innerHTML = `<span></span>${escapeHtml(message)}`;
      setStatus("error", "catalog JSON error");
      renderError(message, "Fix Catalog JSON to resume completion.");
    }
    return undefined;
  }
}

function scheduleAnalysis(delay = 90): void {
  analysisGeneration += 1;
  const generation = analysisGeneration;
  if (analysisTimer !== undefined) window.clearTimeout(analysisTimer);
  analysisTimer = window.setTimeout(() => void refreshAnalysis(generation), delay);
}

async function refreshAnalysis(generation: number): Promise<void> {
  const catalog = parseCatalog();
  if (!catalog) return;
  const source = sqlModel.getValue();
  const position = sqlEditor.getPosition() ?? { lineNumber: 1, column: 1 };
  const cursorUtf16 = sqlModel.getOffsetAt(position);
  setStatus("running", "collecting");
  try {
    const result = await client.complete(source, cursorUtf16, catalog);
    if (generation !== analysisGeneration) return;
    applyResult(result);
  } catch (error) {
    if (generation !== analysisGeneration) return;
    const message = error instanceof Error ? error.message : String(error);
    setStatus("error", "adapter error");
    catalogState.className = "catalog-state invalid";
    catalogState.innerHTML = `<span></span>${escapeHtml(message)}`;
    renderError(message, "The Rust adapter rejected this request.");
  }
}

function applyResult(result: TimedCompletion): void {
  lastResult = result.completion;
  candidateCount.textContent = String(lastResult.items.length);
  timing.textContent = `${formatDuration(result.elapsedMs)} worker round-trip`;
  sqlMeta.textContent = `${lastResult.items.length} candidates · ${formatDuration(result.elapsedMs)}`;
  const recoveryCount = lastResult.context.recovery.length;
  setStatus(recoveryCount > 0 ? "warning" : "ready", recoveryCount > 0 ? `${recoveryCount} recovery issue${recoveryCount === 1 ? "" : "s"}` : "ready");
  catalogState.className = "catalog-state valid";
  catalogState.innerHTML = "<span></span>valid";
  renderInspector();
}

function setStatus(state: "running" | "ready" | "warning" | "error", text: string): void {
  status.dataset.state = state;
  statusText.textContent = text;
}

function renderInspector(): void {
  if (!lastResult) return;
  switch (activeTab) {
    case "candidates":
      renderCandidates(lastResult.items);
      break;
    case "intent":
      renderIntent(lastResult.context);
      break;
    case "scope":
      renderScope(lastResult.context);
      break;
    case "raw":
      inspector.innerHTML = `<pre class="raw-view">${escapeHtml(JSON.stringify(lastResult, null, 2))}</pre>`;
      break;
  }
}

function renderCandidates(items: CompletionItemDto[]): void {
  if (items.length === 0) {
    inspector.innerHTML = `
      <div class="empty-state">
        <div class="empty-glyph">∅</div>
        <p>No candidates at this point.</p>
        <small>The context tabs still show what the parser observed.</small>
      </div>`;
    return;
  }
  const groups = groupBy(items, (item) => item.origin);
  inspector.innerHTML = [...groups.entries()]
    .map(
      ([origin, group]) => `
        <section class="candidate-group">
          <div class="group-heading"><span>${escapeHtml(origin)}</span><b>${group.length}</b></div>
          <div class="candidate-list">
            ${group
              .map(
                (item) => `
                  <button class="candidate-row" type="button" data-candidate="${items.indexOf(item)}">
                    <span class="kind-icon kind-${escapeHtml(item.kind)}">${escapeHtml(kindAbbreviation(item.kind))}</span>
                    <span class="candidate-copy">
                      <strong>${escapeHtml(item.label)}</strong>
                      <small>${escapeHtml(item.detail)}</small>
                    </span>
                    <span class="insert-preview">${escapeHtml(item.insertText)}</span>
                  </button>`,
              )
              .join("")}
          </div>
        </section>`,
    )
    .join("");

  inspector.querySelectorAll<HTMLButtonElement>("[data-candidate]").forEach((button) => {
    button.addEventListener("click", () => {
      const index = Number(button.dataset.candidate);
      const item = items[index];
      if (item) applyCandidate(item);
    });
  });
}

function renderIntent(context: ContextDto): void {
  const qualifier = context.intent.qualifier.map((part) => part.text).join(".") || "—";
  const replacement = context.replacementRange;
  inspector.innerHTML = `
    ${renderRecovery(context)}
    <div class="metric-grid">
      ${metric("Prefix", context.prefix.raw || "∅", `${context.prefix.quoting} · normalized ${context.prefix.normalized || "∅"}`)}
      ${metric("Qualifier", qualifier, `${context.intent.qualifier.length} completed part${context.intent.qualifier.length === 1 ? "" : "s"}`)}
      ${metric("Point", `UTF‑16 ${context.point.effectiveUtf16}`, `UTF‑8 ${context.point.utf8}${context.point.adjusted ? " · adjusted" : ""}`)}
      ${metric("Replacement", `${replacement.utf16.start}…${replacement.utf16.end}`, `UTF‑8 ${replacement.utf8.start}…${replacement.utf8.end}`)}
    </div>
    ${pillSection("Grammar slots", context.expectations.slots)}
    ${pillSection("Object intent", context.intent.objectKinds)}
    ${pillSection("Tokens", context.expectations.tokens)}
    ${pillSection("Direct syntax", context.expectations.directTokens)}
    ${pillSection("Lookahead syntax", context.expectations.lookaheadTokens)}
    ${pillSection("Expression starts", context.expectations.expressionStartTokens)}
    ${pillSection("Expression continuations", context.expectations.expressionContinuationTokens)}
    ${pillSection("Expression follows", context.expectations.followTokens)}
    ${pillSection("Phrases", context.expectations.phrases)}
    ${
      context.intent.container
        ? `<section class="data-section">
            <div class="section-title">Container</div>
            <div class="container-card">
              <strong>${escapeHtml(context.intent.container.name.map((part) => part.text).join("."))}</strong>
              <span>${escapeHtml(context.intent.container.objectKinds.join(" / "))}</span>
              <small>members: ${escapeHtml(context.intent.container.members.join(", "))}</small>
            </div>
          </section>`
        : ""
    }
  `;
}

function renderScope(context: ContextDto): void {
  const scope = context.scope;
  inspector.innerHTML = `
    ${renderRecovery(context)}
    ${relationSection("Local scope", scope.local)}
    ${
      scope.dmlTarget
        ? relationSection("DML target", [scope.dmlTarget])
        : ""
    }
    ${
      scope.mergeSource
        ? relationSection("MERGE source", [scope.mergeSource])
        : ""
    }
    ${scope.outer.map((relations, index) => relationSection(`Outer scope ${index + 1}`, relations)).join("")}
    <section class="data-section">
      <div class="section-title">Visible CTEs <b>${scope.ctes.length}</b></div>
      ${
        scope.ctes.length
          ? scope.ctes
              .map(
                (cte) => `<div class="relation-card"><strong>${escapeHtml(cte.name.text)}</strong><small>${escapeHtml(cte.explicitColumns.map((part) => part.text).join(", ") || "derived output")}</small></div>`,
              )
              .join("")
          : '<div class="inline-empty">No visible CTEs</div>'
      }
    </section>
  `;
}

function renderRecovery(context: ContextDto): string {
  if (context.recovery.length === 0) return "";
  return `<div class="recovery-banner"><strong>Recovered input</strong>${context.recovery
    .map(
      (issue) =>
        `<span>${escapeHtml(issue.kind)} · UTF‑8 ${issue.range.utf8.start}…${issue.range.utf8.end}</span>`,
    )
    .join("")}</div>`;
}

function relationSection(title: string, relations: RelationDto[]): string {
  return `<section class="data-section">
    <div class="section-title">${escapeHtml(title)} <b>${relations.length}</b></div>
    ${
      relations.length
        ? relations.map(renderRelation).join("")
        : '<div class="inline-empty">No visible relations</div>'
    }
  </section>`;
}

function renderRelation(relation: RelationDto): string {
  const name = relation.name.map((part) => part.text).join(".") || "anonymous";
  return `<div class="relation-card ${relation.unsupported ? "unsupported" : ""}">
    <div class="relation-main">
      <strong>${escapeHtml(name)}</strong>
      ${relation.alias ? `<span>AS ${escapeHtml(relation.alias.text)}</span>` : ""}
      <em>${escapeHtml(relation.kind)}</em>
    </div>
    <div class="relation-flags">
      ${relation.lateral ? "<span>LATERAL</span>" : ""}
      ${relation.qualifiedOnly ? "<span>QUALIFIED ONLY</span>" : ""}
      ${relation.explicitColumns.length ? `<span>${relation.explicitColumns.length} explicit columns</span>` : ""}
    </div>
    ${relation.unsupported ? `<small>${escapeHtml(relation.unsupported.reason)}</small>` : ""}
  </div>`;
}

function metric(label: string, value: string, detail: string): string {
  return `<div class="metric"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong><small>${escapeHtml(detail)}</small></div>`;
}

function pillSection(title: string, values: string[]): string {
  return `<section class="data-section"><div class="section-title">${escapeHtml(title)} <b>${values.length}</b></div><div class="pill-list">${
    values.length
      ? values.map((value) => `<span>${escapeHtml(value)}</span>`).join("")
      : '<em class="inline-empty">empty</em>'
  }</div></section>`;
}

function renderError(message: string, hint: string): void {
  inspector.innerHTML = `
    <div class="error-state">
      <div class="error-symbol">!</div>
      <strong>${escapeHtml(message)}</strong>
      <p>${escapeHtml(hint)}</p>
    </div>`;
}

function applyCandidate(item: CompletionItemDto): void {
  const start = sqlModel.getPositionAt(item.replacementRange.start);
  const end = sqlModel.getPositionAt(item.replacementRange.end);
  const range = new monaco.Range(
    start.lineNumber,
    start.column,
    end.lineNumber,
    end.column,
  );
  sqlEditor.executeEdits("pg-completion-playground", [
    { range, text: item.insertText, forceMoveMarkers: true },
  ]);
  const cursor = sqlModel.getPositionAt(item.replacementRange.start + item.insertText.length);
  sqlEditor.setPosition(cursor);
  sqlEditor.focus();
  if (item.triggerSuggest) {
    void sqlEditor.trigger("pg-completion-playground", "editor.action.triggerSuggest", {});
  }
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

monaco.languages.registerCompletionItemProvider("sql", {
  triggerCharacters: [".", '"', " "],
  async provideCompletionItems(model, position, _completionContext, cancellationToken) {
    if (model !== sqlModel) return { suggestions: [] };
    const catalog = parseCatalog(false);
    if (!catalog) return { suggestions: [] };
    const source = model.getValue();
    const cursorUtf16 = model.getOffsetAt(position);
    try {
      const result = await client.complete(source, cursorUtf16, catalog);
      if (cancellationToken.isCancellationRequested) return { suggestions: [] };
      lastResult = result.completion;
      candidateCount.textContent = String(lastResult.items.length);
      timing.textContent = `${formatDuration(result.elapsedMs)} worker round-trip`;
      renderInspector();
      return {
        suggestions: result.completion.items.map((item) => {
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
            documentation: `Origin: ${item.origin}${item.objectKind ? ` · ${item.objectKind}` : ""}`,
            sortText: item.sortText,
            filterText: item.label,
            command: item.triggerSuggest
              ? { id: "editor.action.triggerSuggest", title: "Suggest members" }
              : undefined,
          } satisfies monaco.languages.CompletionItem;
        }),
      };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus("error", "completion failed");
      renderError(message, "Inspect Catalog JSON and try again.");
      return { suggestions: [] };
    }
  },
});

function loadExample(index: number): void {
  const example = EXAMPLES[index] ?? EXAMPLES[0];
  if (!example) return;
  const marker = example.source.indexOf("|");
  const source = example.source.replace("|", "");
  sqlModel.setValue(source);
  const point = marker >= 0 ? marker : source.length;
  const position = sqlModel.getPositionAt(point);
  sqlEditor.setPosition(position);
  sqlEditor.revealPositionInCenterIfOutsideViewport(position);
  scenarioNote.textContent = example.description;
  updateCursorPosition();
  scheduleAnalysis(0);
}

function updateCursorPosition(): void {
  const position = sqlEditor.getPosition();
  if (!position) return;
  const offset = sqlModel.getOffsetAt(position);
  cursorPosition.textContent = `Ln ${position.lineNumber}, Col ${position.column} · UTF‑16 ${offset}`;
}

sqlEditor.onDidChangeModelContent(() => scheduleAnalysis());
sqlEditor.onDidChangeCursorPosition(() => {
  updateCursorPosition();
  scheduleAnalysis(70);
});
catalogEditor.onDidChangeModelContent(() => {
  if (parseCatalog()) scheduleAnalysis(180);
});

element<HTMLButtonElement>("#trigger-completion").addEventListener("click", () => {
  sqlEditor.focus();
  void sqlEditor.trigger("toolbar", "editor.action.triggerSuggest", {});
});
element<HTMLButtonElement>("#reset-catalog").addEventListener("click", () => {
  catalogModel.setValue(DEFAULT_CATALOG);
  scheduleAnalysis(0);
});
exampleSelect.addEventListener("change", () => loadExample(Number(exampleSelect.value)));

document.querySelectorAll<HTMLButtonElement>(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    activeTab = tab.dataset.tab ?? "candidates";
    document.querySelectorAll(".tab").forEach((candidate) => {
      candidate.classList.toggle("active", candidate === tab);
    });
    renderInspector();
  });
});

window.addEventListener("beforeunload", () => {
  client.dispose();
  sqlEditor.dispose();
  catalogEditor.dispose();
  sqlModel.dispose();
  catalogModel.dispose();
});

function groupBy<T, K>(values: T[], key: (value: T) => K): Map<K, T[]> {
  const groups = new Map<K, T[]>();
  for (const value of values) {
    const groupKey = key(value);
    const group = groups.get(groupKey);
    if (group) group.push(value);
    else groups.set(groupKey, [value]);
  }
  return groups;
}

function kindAbbreviation(kind: string): string {
  const abbreviations: Record<string, string> = {
    column: "C",
    function: "ƒ",
    table: "T",
    schema: "S",
    type: "τ",
    reference: "R",
    keyword: "K",
    phrase: "P",
    privilege: "G",
    object: "O",
    user: "U",
    database: "D",
  };
  return abbreviations[kind] ?? kind.slice(0, 1).toUpperCase();
}

function formatDuration(milliseconds: number): string {
  return milliseconds < 1
    ? `${Math.max(1, Math.round(milliseconds * 1000))} µs`
    : `${milliseconds.toFixed(milliseconds < 10 ? 1 : 0)} ms`;
}

function escapeHtml(value: string): string {
  return value.replace(
    /[&<>'"]/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        "'": "&#39;",
        '"': "&quot;",
      })[character] ?? character,
  );
}

loadExample(0);
sqlEditor.focus();
