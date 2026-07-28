<script setup lang="ts">
import {
  PhCode,
  PhCrosshair,
  PhDatabase,
} from "@phosphor-icons/vue";
import { ref } from "vue";

import AppHeader from "./components/AppHeader.vue";
import InspectorPanel from "./components/InspectorPanel.vue";
import MonacoEditor from "./components/MonacoEditor.vue";
import { DEFAULT_CATALOG } from "./data";
import { usePlayground } from "./composables/usePlayground";
import { useTheme } from "./composables/useTheme";

const {
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
} = usePlayground();

const { theme, toggleTheme } = useTheme();
const mobilePane = ref<"query" | "catalog" | "context">("query");
</script>

<template>
  <div class="app-shell">
    <AppHeader :theme="theme" @toggle-theme="toggleTheme" />

    <main class="workspace">
      <div :class="['editor-stack', { 'mobile-hidden': mobilePane === 'context' }]">
        <section
          :class="['work-panel', 'query-panel', { 'mobile-active': mobilePane === 'query' }]"
          aria-label="SQL query editor"
        >
          <header class="panel-header">
            <div class="panel-title">
              <PhCode :size="17" />
              <div>
                <h2>Query</h2>
                <span>query.sql</span>
              </div>
            </div>
            <span class="panel-meta">{{ sqlMeta }}</span>
          </header>
          <MonacoEditor
            language="sql"
            uri="inmemory://playground/query.sql"
            initial-value=""
            :theme="theme"
            @ready="attachSql"
            @change="onSqlChange"
            @cursor="onSqlCursor"
          />
        </section>

        <section
          :class="['work-panel', 'catalog-panel', { 'mobile-active': mobilePane === 'catalog' }]"
          aria-label="Catalog JSON editor"
        >
          <header class="panel-header">
            <div class="panel-title">
              <PhDatabase :size="17" />
              <div>
                <h2>Catalog</h2>
                <span>catalog.json</span>
              </div>
            </div>
            <span
              :class="['catalog-validation', { invalid: !catalogValid }]"
              :title="catalogMessage"
            >
              {{ catalogValid ? "Valid JSON" : "Invalid JSON" }}
            </span>
          </header>
          <MonacoEditor
            language="json"
            uri="inmemory://playground/catalog.json"
            :initial-value="DEFAULT_CATALOG"
            :theme="theme"
            compact
            @ready="attachCatalog"
            @change="onCatalogChange"
          />
        </section>
      </div>

      <InspectorPanel
        :class="{ 'mobile-active': mobilePane === 'context' }"
        :result="result"
        :status="status"
        :timing="timing"
        @apply="applyCandidate"
      />
    </main>

    <nav class="mobile-nav" aria-label="Workspace panels">
      <button
        type="button"
        :class="{ active: mobilePane === 'query' }"
        :aria-pressed="mobilePane === 'query'"
        @click="mobilePane = 'query'"
      >
        <PhCode :size="17" />
        Query
      </button>
      <button
        type="button"
        :class="{ active: mobilePane === 'catalog' }"
        :aria-pressed="mobilePane === 'catalog'"
        @click="mobilePane = 'catalog'"
      >
        <PhDatabase :size="17" />
        Catalog
      </button>
      <button
        type="button"
        :class="{ active: mobilePane === 'context' }"
        :aria-pressed="mobilePane === 'context'"
        @click="mobilePane = 'context'"
      >
        <PhCrosshair :size="17" />
        Context
      </button>
    </nav>
  </div>
</template>
