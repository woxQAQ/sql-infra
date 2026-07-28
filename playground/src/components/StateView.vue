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
