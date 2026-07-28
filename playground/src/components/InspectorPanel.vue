<script setup lang="ts">
import {
  PhCode,
  PhCrosshair,
  PhList,
  PhTreeStructure,
} from "@phosphor-icons/vue";
import { ref } from "vue";

import type { AnalysisStatus } from "../composables/usePlayground";
import type { CompletionItemDto, CompletionResponseDto } from "../types";
import CandidatesView from "./CandidatesView.vue";
import IntentView from "./IntentView.vue";
import ScopeView from "./ScopeView.vue";
import StateView from "./StateView.vue";

defineProps<{
  result?: CompletionResponseDto;
  status: AnalysisStatus;
  timing: string;
}>();

defineEmits<{
  apply: [item: CompletionItemDto];
}>();

type InspectorTab = "candidates" | "intent" | "scope" | "raw";

const activeTab = ref<InspectorTab>("candidates");
const tabs = [
  { id: "candidates", label: "Candidates", icon: PhList },
  { id: "intent", label: "Intent", icon: PhCrosshair },
  { id: "scope", label: "Scope", icon: PhTreeStructure },
  { id: "raw", label: "Raw", icon: PhCode },
] as const;
</script>

<template>
  <aside class="inspector-panel" aria-label="Completion inspector">
    <header class="panel-header inspector-titlebar">
      <div class="panel-title">
        <PhCrosshair :size="17" />
        <div>
          <h2>Completion context</h2>
          <span>Live parser result</span>
        </div>
      </div>
      <span class="analysis-state" :data-state="status.state">{{ status.label }}</span>
    </header>

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
        <b v-if="tab.id === 'candidates'">{{ result?.items.length ?? 0 }}</b>
      </button>
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
      <pre v-else class="raw-view">{{ JSON.stringify(result, null, 2) }}</pre>
    </div>

    <footer class="inspector-footer">
      <span>{{ timing }}</span>
      <span>UTF-16 and UTF-8 offsets</span>
    </footer>
  </aside>
</template>
