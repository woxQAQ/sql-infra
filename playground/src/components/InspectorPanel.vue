<script setup lang="ts">
import {
  PhCode,
  PhCrosshair,
  PhDatabase,
  PhList,
  PhTreeStructure,
} from "@phosphor-icons/vue";
import { ref } from "vue";

import type { AnalysisStatus } from "../composables/usePlayground";
import type { CatalogDocument, CompletionItemDto, CompletionResponseDto } from "../types";
import CandidatesView from "./CandidatesView.vue";
import CatalogView from "./CatalogView.vue";
import IntentView from "./IntentView.vue";
import ScopeView from "./ScopeView.vue";
import StateView from "./StateView.vue";

defineProps<{
  result?: CompletionResponseDto;
  status: AnalysisStatus;
  catalog: CatalogDocument;
}>();

defineEmits<{
  apply: [item: CompletionItemDto];
}>();

type InspectorTab = "candidates" | "intent" | "scope" | "catalog" | "raw";

const activeTab = ref<InspectorTab>("candidates");
const tabs = [
  { id: "candidates", label: "Candidates", icon: PhList },
  { id: "intent", label: "Intent", icon: PhCrosshair },
  { id: "scope", label: "Scope", icon: PhTreeStructure },
  { id: "catalog", label: "Catalog", icon: PhDatabase },
  { id: "raw", label: "Raw", icon: PhCode },
] as const;
</script>

<template>
  <aside class="inspector-panel" aria-label="Completion inspector">
    <nav class="inspector-tabs" aria-label="Inspector views">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        :class="['inspector-tab', { active: activeTab === tab.id }]"
        :aria-pressed="activeTab === tab.id"
        @click="activeTab = tab.id"
      >
        <component :is="tab.icon" :size="14" />
        {{ tab.label }}
        <!-- <b v-if="tab.id === 'candidates'">{{ result?.items.length ?? 0 }}</b> -->
      </button>
      <span
        v-if="status.state !== 'ready'"
        class="analysis-state"
        :data-state="status.state"
      >
        {{ status.label }}
      </span>
    </nav>

    <div class="inspector-content">
      <StateView
        v-if="status.state === 'error'"
        state="error"
        :title="status.label"
        :detail="status.detail ?? 'Inspect the request and try again.'"
      />
      <StateView
        v-else-if="!result"
        state="loading"
        title="Loading completion engine"
        detail="Rust and WASM are starting in a dedicated worker."
      />
      <CandidatesView
        v-else-if="activeTab === 'candidates'"
        :items="result.items"
        @apply="$emit('apply', $event)"
      />
      <IntentView v-else-if="activeTab === 'intent'" :context="result.context" />
      <ScopeView v-else-if="activeTab === 'scope'" :context="result.context" />
      <CatalogView v-else-if="activeTab === 'catalog'" :catalog="catalog" />
      <pre v-else class="raw-view">{{ JSON.stringify(result, null, 2) }}</pre>
    </div>

  </aside>
</template>

<style scoped>
.inspector-panel {
  display: grid;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  grid-template-rows: 48px minmax(0, 1fr);
  border: 1px solid var(--border);
  border-radius: var(--panel-radius);
  background: var(--surface);
  box-shadow: 0 12px 30px var(--shadow);
}

.inspector-tabs {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr)) auto;
  border-bottom: 1px solid var(--border);
  background: var(--surface-raised);
}

.inspector-tab {
  position: relative;
  display: flex;
  min-width: 0;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 0;
  background: transparent;
  color: var(--text-dim);
  font-size: 10px;
  font-weight: 650;
  transition: color 140ms ease, background 140ms ease;
}

.inspector-tab::after {
  position: absolute;
  right: 14px;
  bottom: -1px;
  left: 14px;
  height: 2px;
  border-radius: 2px 2px 0 0;
  background: transparent;
  content: "";
}

.inspector-tab:hover,
.inspector-tab.active {
  background: var(--accent-soft);
  color: var(--text);
}

.inspector-tab.active::after {
  background: var(--accent);
}

.inspector-tab b {
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 9px;
  font-weight: 550;
}

.analysis-state {
  align-self: center;
  margin: 0 12px 0 8px;
  padding: 6px 9px;
  border: 1px solid var(--border);
  border-radius: 7px;
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  white-space: nowrap;
}

.analysis-state[data-state="warning"],
.analysis-state[data-state="running"] {
  border-color: color-mix(in srgb, var(--warning) 48%, var(--border));
  background: var(--warning-surface);
  color: var(--warning);
}

.analysis-state[data-state="error"] {
  border-color: color-mix(in srgb, var(--danger) 48%, var(--border));
  background: var(--danger-surface);
  color: var(--danger);
}

.inspector-content {
  position: relative;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  contain: paint;
  scrollbar-color: var(--border-strong) transparent;
  scrollbar-width: thin;
}

.raw-view {
  min-height: 100%;
  margin: 0;
  padding: 18px;
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
}

@media (max-width: 760px) {
  .inspector-panel {
    display: none;
    width: 100%;
    height: 100%;
    border-radius: 12px;
  }

  .inspector-panel.mobile-active {
    display: grid;
  }
}

@media (max-width: 480px) {
  .inspector-tab {
    gap: 3px;
    font-size: 9px;
  }

  .analysis-state {
    margin: 0 7px 0 4px;
    padding: 5px 6px;
    font-size: 9px;
  }
}
</style>
