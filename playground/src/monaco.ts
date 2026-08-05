import * as monaco from "monaco-editor/esm/vs/editor/editor.api.js";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker.js?worker";
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

self.MonacoEnvironment = {
  getWorker() {
    return new EditorWorker();
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
      { token: "keyword.sql", foreground: "A9BFFF", fontStyle: "bold" },
      { token: "string.sql", foreground: "A7C9C3" },
      { token: "number.sql", foreground: "E0B584" },
      { token: "comment.sql", foreground: "77849A", fontStyle: "italic" },
      { token: "delimiter.sql", foreground: "A8B4C8" },
    ],
    colors: {
      "editor.background": "#141B29",
      "editor.foreground": "#E9EEF8",
      "editorLineNumber.foreground": "#56657D",
      "editorLineNumber.activeForeground": "#A8B4C8",
      "editorCursor.foreground": "#ABC0FF",
      "editor.selectionBackground": "#526AAB59",
      "editor.inactiveSelectionBackground": "#526AAB38",
      "editor.lineHighlightBackground": "#182131",
      "editorSuggestWidget.background": "#182131",
      "editorSuggestWidget.border": "#3C4D69",
      "editorSuggestWidget.foreground": "#E9EEF8",
      "editorSuggestWidget.selectedBackground": "#202F4C",
      "editorSuggestWidget.selectedForeground": "#E9EEF8",
      "editorSuggestWidget.selectedIconForeground": "#ABC0FF",
      "editorSuggestWidget.highlightForeground": "#ABC0FF",
      "editorSuggestWidget.focusHighlightForeground": "#ABC0FF",
      "editorWidget.background": "#182131",
      "editorWidget.border": "#3C4D69",
      "input.background": "#141B29",
      "input.border": "#3C4D69",
      "focusBorder": "#89A8FF",
      "scrollbarSlider.background": "#77849A3D",
      "scrollbarSlider.hoverBackground": "#8C97A84F",
    },
  });

  monaco.editor.defineTheme("pg-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword.sql", foreground: "3659B2", fontStyle: "bold" },
      { token: "string.sql", foreground: "287064" },
      { token: "number.sql", foreground: "9A642E" },
      { token: "comment.sql", foreground: "8C97A8", fontStyle: "italic" },
      { token: "delimiter.sql", foreground: "5F6B7D" },
    ],
    colors: {
      "editor.background": "#FFFFFF",
      "editor.foreground": "#172033",
      "editorLineNumber.foreground": "#A5AFBF",
      "editorLineNumber.activeForeground": "#5F6B7D",
      "editorCursor.foreground": "#3659B2",
      "editor.selectionBackground": "#B7C7F266",
      "editor.inactiveSelectionBackground": "#B7C7F238",
      "editor.lineHighlightBackground": "#F4F6FA",
      "editorSuggestWidget.background": "#FBFCFE",
      "editorSuggestWidget.border": "#DCE2EC",
      "editorSuggestWidget.foreground": "#172033",
      "editorSuggestWidget.selectedBackground": "#EEF2FF",
      "editorSuggestWidget.selectedForeground": "#172033",
      "editorSuggestWidget.selectedIconForeground": "#3659B2",
      "editorSuggestWidget.highlightForeground": "#3659B2",
      "editorSuggestWidget.focusHighlightForeground": "#3659B2",
      "editorWidget.background": "#FBFCFE",
      "editorWidget.border": "#DCE2EC",
      "input.background": "#FFFFFF",
      "input.border": "#DCE2EC",
      "focusBorder": "#5274C9",
      "scrollbarSlider.background": "#8C97A82B",
      "scrollbarSlider.hoverBackground": "#8C97A845",
    },
  });

}

export type MonacoEditor = monaco.editor.IStandaloneCodeEditor;
export type MonacoModel = monaco.editor.ITextModel;
export { monaco };
