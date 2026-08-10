<script setup lang="ts">
import { PhMoon, PhSun } from "@phosphor-icons/vue";

defineProps<{
  theme: "light" | "dark";
}>();

defineEmits<{
  toggleTheme: [];
}>();
</script>

<template>
  <header class="app-header">
    <div class="brand" aria-label="pg-completion playground">
      <!-- <span class="brand-mark"><PhBracketsCurly :size="18" weight="bold" /></span> -->
      <!-- <div class="wordmark"> -->
        <!-- <span class="wordmark-prefix">pg</span><span>/completion</span> -->
        <!-- <b>playground</b> -->
      <!-- </div> -->
    </div>
    <div class="header-actions">
      <button
        class="icon-button"
        type="button"
        :aria-label="theme === 'dark' ? 'Use light theme' : 'Use dark theme'"
        :title="theme === 'dark' ? 'Use light theme' : 'Use dark theme'"
        @click="$emit('toggleTheme')"
      >
        <PhSun v-if="theme === 'dark'" :size="17" weight="regular" />
        <PhMoon v-else :size="17" weight="regular" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.app-header {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 24px;
  padding: 0 clamp(18px, 3vw, 42px);
  border-bottom: 1px solid var(--border);
  background: color-mix(in srgb, var(--surface) 90%, transparent);
  box-shadow: 0 1px 0 var(--surface);
}

.brand {
  display: flex;
  align-items: center;
  gap: 11px;
}

.brand-mark {
  display: grid;
  width: 34px;
  height: 34px;
  place-items: center;
  border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--border));
  border-radius: 10px;
  background: var(--accent-surface);
  color: var(--accent-strong);
}

.wordmark {
  display: flex;
  align-items: baseline;
  color: var(--text);
  font-size: 14px;
  font-weight: 760;
  letter-spacing: -0.03em;
  white-space: nowrap;
}

.wordmark-prefix {
  color: var(--accent-strong);
}

.wordmark b {
  margin-left: 8px;
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.05em;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-self: end;
}

.icon-button {
  display: grid;
  width: 36px;
  height: 36px;
  place-items: center;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--surface-raised);
  transition: background 140ms ease, border-color 140ms ease, transform 140ms ease;
}

.icon-button:hover {
  border-color: var(--border-strong);
  background: var(--surface-active);
}

.icon-button:active {
  transform: translateY(1px);
}

@media (max-width: 760px) {
  .app-header {
    gap: 10px;
    padding: 0 14px;
  }

  .wordmark b {
    display: none;
  }

  .brand-mark {
    width: 32px;
    height: 32px;
  }
}
</style>
