<script setup lang="ts">
import {
  PhCode,
  PhCrosshair,
} from "@phosphor-icons/vue";
import { ref } from "vue";

import AppHeader from "./components/AppHeader.vue";
import InspectorPanel from "./components/InspectorPanel.vue";
import MonacoEditor from "./components/MonacoEditor.vue";
import { DEFAULT_CATALOG_DOCUMENT } from "./data";
import { usePlayground } from "./composables/usePlayground";
import { useTheme } from "./composables/useTheme";

const {
  result,
  status,
  attachSql,
  applyCandidate,
  onSqlChange,
  onSqlCursor,
} = usePlayground();

const { theme, toggleTheme } = useTheme();
const mobilePane = ref<"query" | "context">("query");
</script>

<template>
  <div class="app-shell">
    <AppHeader :theme="theme" @toggle-theme="toggleTheme" />

    <main class="workspace-shell">
      <div class="workspace">
        <div :class="['editor-stack', { 'mobile-hidden': mobilePane === 'context' }]">
          <section
            :class="['work-panel', 'query-panel', { 'mobile-active': mobilePane === 'query' }]"
            aria-label="SQL query editor"
          >
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
        </div>

        <InspectorPanel
          :class="{ 'mobile-active': mobilePane === 'context' }"
          :result="result"
          :status="status"
          :catalog="DEFAULT_CATALOG_DOCUMENT"
          @apply="applyCandidate"
        />
      </div>
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

<style>
:root {
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  --font-mono: "SFMono-Regular", "Cascadia Code", "JetBrains Mono", Consolas, monospace;
  --app-bg: #f3f5f9;
  --surface: #ffffff;
  --surface-raised: #fbfcfe;
  --surface-muted: #f4f6fa;
  --surface-active: #eef2ff;
  --border: #dce2ec;
  --border-strong: #c4ccda;
  --text: #172033;
  --text-muted: #5f6b7d;
  --text-dim: #8c97a8;
  --accent: #5274c9;
  --accent-strong: #3659b2;
  --accent-surface: #e7edff;
  --accent-soft: #f4f6ff;
  --danger: #b14f4c;
  --danger-surface: #fbe9e6;
  --warning: #9a701b;
  --warning-surface: #fff5d8;
  --shadow: rgba(34, 50, 79, 0.08);
  --shadow-strong: rgba(34, 50, 79, 0.13);
  --panel-radius: 14px;
  --control-radius: 8px;
}

:root[data-theme="dark"] {
  --app-bg: #0d121c;
  --surface: #141b29;
  --surface-raised: #182131;
  --surface-muted: #1c2638;
  --surface-active: #202f4c;
  --border: #2a3850;
  --border-strong: #3c4d69;
  --text: #e9eef8;
  --text-muted: #a8b4c8;
  --text-dim: #77849a;
  --accent: #89a8ff;
  --accent-strong: #abc0ff;
  --accent-surface: #233354;
  --accent-soft: #1a2944;
  --danger: #ef9188;
  --danger-surface: #38211f;
  --warning: #e2bb63;
  --warning-surface: #382d18;
  --shadow: rgba(0, 0, 0, 0.22);
  --shadow-strong: rgba(0, 0, 0, 0.38);
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  width: 100%;
  height: 100%;
  margin: 0;
}

body {
  min-width: 320px;
  overflow: hidden;
  background: var(--app-bg);
  color: var(--text);
}

button,
select {
  color: inherit;
  font: inherit;
}

button {
  cursor: pointer;
}

button:focus-visible,
select:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 3px;
}
</style>

<style scoped>
.app-shell {
  display: grid;
  grid-template-rows: 68px minmax(0, 1fr);
  width: 100%;
  height: 100%;
  min-height: 100dvh;
}

.workspace-shell {
  display: grid;
  min-width: 0;
  min-height: 0;
  grid-template-rows: minmax(0, 1fr);
  padding: 24px clamp(18px, 3vw, 42px) 28px;
  background:
    radial-gradient(circle at 14% 0%, color-mix(in srgb, var(--accent-surface) 65%, transparent), transparent 32%),
    var(--app-bg);
}

.workspace {
  display: grid;
  min-width: 0;
  min-height: 0;
  grid-template-columns: minmax(0, 1fr) minmax(360px, 0.42fr);
  gap: 16px;
}

.editor-stack {
  display: grid;
  min-width: 0;
  min-height: 0;
  grid-template-rows: minmax(0, 1fr);
}

.work-panel {
  display: grid;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  grid-template-rows: minmax(0, 1fr);
  border: 1px solid var(--border);
  border-radius: var(--panel-radius);
  background: var(--surface);
  box-shadow: 0 16px 36px var(--shadow-strong);
}

.query-panel {
  border-color: color-mix(in srgb, var(--accent) 26%, var(--border));
}

.mobile-nav {
  display: none;
}

@media (max-width: 1120px) {
  .workspace {
    grid-template-columns: minmax(0, 1fr) minmax(330px, 0.48fr);
  }
}

@media (max-width: 760px) {
  .app-shell {
    grid-template-rows: 58px minmax(0, 1fr) 58px;
  }

  .workspace-shell {
    display: block;
    padding: 8px;
  }

  .workspace {
    display: block;
    width: 100%;
    height: 100%;
  }

  .editor-stack {
    display: block;
    width: 100%;
    height: 100%;
  }

  .editor-stack.mobile-hidden {
    display: none;
  }

  .work-panel {
    display: none;
    width: 100%;
    height: 100%;
    border-radius: 12px;
  }

  .work-panel.mobile-active {
    display: grid;
  }

  .mobile-nav {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    border-top: 1px solid var(--border);
    background: var(--surface);
  }

  .mobile-nav button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 0;
    border-right: 1px solid var(--border);
    background: transparent;
    color: var(--text-dim);
    font-size: 10px;
    font-weight: 680;
  }

  .mobile-nav button:last-child {
    border-right: 0;
  }

  .mobile-nav button.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
}

@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    scroll-behavior: auto !important;
    transition-duration: 0.01ms !important;
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
  }
}
</style>
