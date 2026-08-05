<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";

import {
  configureMonaco,
  monaco,
  type MonacoEditor,
  type MonacoModel,
} from "../monaco";

const props = defineProps<{
  language: "sql";
  uri: string;
  initialValue: string;
  theme: "light" | "dark";
}>();

const emit = defineEmits<{
  ready: [editor: MonacoEditor, model: MonacoModel];
  change: [];
  cursor: [];
}>();

const host = ref<HTMLDivElement>();
let editor: MonacoEditor | undefined;
let model: MonacoModel | undefined;
const disposables: monaco.IDisposable[] = [];

onMounted(() => {
  if (!host.value) return;
  configureMonaco();
  model = monaco.editor.createModel(
    props.initialValue,
    props.language,
    monaco.Uri.parse(props.uri),
  );
  editor = monaco.editor.create(host.value, {
    model,
    theme: props.theme === "dark" ? "pg-dark" : "pg-light",
    automaticLayout: true,
    fontFamily: '"SFMono-Regular", "Cascadia Code", Consolas, monospace',
    fontSize: 13,
    lineHeight: 22,
    fontLigatures: true,
    minimap: { enabled: false },
    overviewRulerLanes: 0,
    hideCursorInOverviewRuler: true,
    renderLineHighlight: "all",
    scrollBeyondLastLine: false,
    smoothScrolling: true,
    padding: { top: 16, bottom: 16 },
    fixedOverflowWidgets: true,
    bracketPairColorization: { enabled: true },
    lineNumbers: "on",
    lineNumbersMinChars: 3,
    folding: true,
    wordWrap: "on",
    suggest: {
      showWords: false,
      showSnippets: false,
      preview: true,
      localityBonus: true,
    },
    quickSuggestions: { other: true, comments: false, strings: false },
    suggestOnTriggerCharacters: true,
  });
  disposables.push(
    model.onDidChangeContent(() => emit("change")),
    editor.onDidChangeCursorPosition(() => emit("cursor")),
  );
  emit("ready", editor, model);
});

watch(
  () => props.theme,
  (theme) => monaco.editor.setTheme(theme === "dark" ? "pg-dark" : "pg-light"),
);

onBeforeUnmount(() => {
  for (const disposable of disposables) disposable.dispose();
  editor?.dispose();
  model?.dispose();
});
</script>

<template>
  <div ref="host" class="monaco-host" />
</template>

<style scoped>
.monaco-host {
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
}
</style>
