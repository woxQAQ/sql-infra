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

import { OBJECT_KINDS } from "./data";

self.MonacoEnvironment = {
  getWorker(_moduleId: string, label: string) {
    return label === "json" ? new JsonWorker() : new EditorWorker();
  },
};

let configured = false;

export function configureMonaco(): void {
  if (configured) return;
  configured = true;

  monaco.editor.defineTheme("pg-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword.sql", foreground: "D6EE78", fontStyle: "bold" },
      { token: "string.sql", foreground: "A7C8B4" },
      { token: "number.sql", foreground: "D6AA7B" },
      { token: "comment.sql", foreground: "697067", fontStyle: "italic" },
      { token: "delimiter.sql", foreground: "8C9389" },
    ],
    colors: {
      "editor.background": "#11130F",
      "editor.foreground": "#E0E3DA",
      "editorLineNumber.foreground": "#54594F",
      "editorLineNumber.activeForeground": "#A7ADA1",
      "editorCursor.foreground": "#D6EE78",
      "editor.selectionBackground": "#596D3659",
      "editor.inactiveSelectionBackground": "#596D3638",
      "editor.lineHighlightBackground": "#171A14",
      "editorSuggestWidget.background": "#171A14",
      "editorSuggestWidget.border": "#353A31",
      "editorSuggestWidget.foreground": "#E0E3DA",
      "editorSuggestWidget.selectedBackground": "#303A23",
      "editorSuggestWidget.selectedForeground": "#E0E3DA",
      "editorSuggestWidget.selectedIconForeground": "#D6EE78",
      "editorSuggestWidget.highlightForeground": "#D6EE78",
      "editorSuggestWidget.focusHighlightForeground": "#D6EE78",
      "editorWidget.background": "#171A14",
      "editorWidget.border": "#353A31",
      "input.background": "#11130F",
      "input.border": "#353A31",
      "focusBorder": "#A7BE53",
      "scrollbarSlider.background": "#68705F3D",
      "scrollbarSlider.hoverBackground": "#7C84734F",
    },
  });

  monaco.editor.defineTheme("pg-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword.sql", foreground: "52630C", fontStyle: "bold" },
      { token: "string.sql", foreground: "38684E" },
      { token: "number.sql", foreground: "925B31" },
      { token: "comment.sql", foreground: "858A7E", fontStyle: "italic" },
      { token: "delimiter.sql", foreground: "6D7268" },
    ],
    colors: {
      "editor.background": "#F7F7F2",
      "editor.foreground": "#252820",
      "editorLineNumber.foreground": "#A0A498",
      "editorLineNumber.activeForeground": "#555A4F",
      "editorCursor.foreground": "#61740D",
      "editor.selectionBackground": "#CADA8066",
      "editor.inactiveSelectionBackground": "#CADA8038",
      "editor.lineHighlightBackground": "#F0F1E9",
      "editorSuggestWidget.background": "#FAFAF6",
      "editorSuggestWidget.border": "#CFD1C7",
      "editorSuggestWidget.foreground": "#252820",
      "editorSuggestWidget.selectedBackground": "#EBEFD8",
      "editorSuggestWidget.selectedForeground": "#252820",
      "editorSuggestWidget.selectedIconForeground": "#52630C",
      "editorSuggestWidget.highlightForeground": "#61740D",
      "editorSuggestWidget.focusHighlightForeground": "#52630C",
      "editorWidget.background": "#FAFAF6",
      "editorWidget.border": "#CFD1C7",
      "input.background": "#F7F7F2",
      "input.border": "#CFD1C7",
      "focusBorder": "#75891D",
      "scrollbarSlider.background": "#757B6B2B",
      "scrollbarSlider.hoverBackground": "#757B6B45",
    },
  });

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
}

export type MonacoEditor = monaco.editor.IStandaloneCodeEditor;
export type MonacoModel = monaco.editor.ITextModel;
export { monaco };
