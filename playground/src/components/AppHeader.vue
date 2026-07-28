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
    <div class="wordmark" aria-label="pg-completion playground">
      <span class="wordmark-prefix">pg</span><span>/completion</span>
      <b>playground</b>
    </div>
    <p>Parser-native PostgreSQL completions</p>
    <div class="header-actions">
      <span class="runtime-state">WASM worker local</span>
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
