<script setup lang="ts">
import {
  PhMagnifyingGlass,
  PhWarning,
} from "@phosphor-icons/vue";

defineProps<{
  state: "loading" | "empty" | "error";
  title: string;
  detail: string;
}>();
</script>

<template>
  <div
    class="state-view"
    :data-state="state"
    :role="state === 'error' ? 'alert' : 'status'"
    :aria-live="state === 'error' ? 'assertive' : 'polite'"
  >
    <div v-if="state === 'loading'" class="state-skeleton" aria-hidden="true">
      <span />
      <span />
      <span />
    </div>
    <PhMagnifyingGlass v-else-if="state === 'empty'" class="state-icon" :size="28" />
    <PhWarning v-else class="state-icon" :size="28" />
    <strong>{{ title }}</strong>
    <p>{{ detail }}</p>
  </div>
</template>

<style scoped>
.state-view {
  display: flex;
  height: 100%;
  min-height: 240px;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  padding: 28px;
  text-align: center;
}

.state-icon {
  margin-bottom: 14px;
  color: var(--accent);
}

.state-skeleton {
  display: grid;
  width: min(100%, 250px);
  gap: 9px;
  margin-bottom: 20px;
}

.state-skeleton span {
  height: 10px;
  border-radius: 4px;
  background: var(--surface-muted);
}

.state-skeleton span:first-child {
  width: 62%;
  background: var(--surface-active);
}

.state-skeleton span:last-child {
  width: 78%;
}

.state-view[data-state="error"] .state-icon,
.state-view[data-state="error"] strong {
  color: var(--danger);
}

.state-view strong {
  color: var(--text);
  font-size: 12px;
}

.state-view p {
  max-width: 310px;
  margin: 7px 0 0;
  color: var(--text-muted);
  font-size: 10px;
  line-height: 1.55;
}
</style>
