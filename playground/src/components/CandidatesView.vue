<script setup lang="ts">
import {
  PhBracketsCurly,
  PhColumns,
  PhCube,
  PhDatabase,
  PhFunction,
  PhTable,
  PhTreeStructure,
  PhUser,
} from "@phosphor-icons/vue";
import { computed, type Component } from "vue";

import type { CompletionItemDto } from "../types";
import StateView from "./StateView.vue";

const props = defineProps<{
  items: CompletionItemDto[];
}>();

defineEmits<{
  apply: [item: CompletionItemDto];
}>();

const groups = computed(() => {
  const grouped = new Map<string, CompletionItemDto[]>();
  for (const item of props.items) {
    const group = grouped.get(item.origin);
    if (group) group.push(item);
    else grouped.set(item.origin, [item]);
  }
  return [...grouped.entries()].map(([name, items]) => ({ name, items }));
});

const iconMap: Record<string, Component> = {
  column: PhColumns,
  function: PhFunction,
  table: PhTable,
  schema: PhTreeStructure,
  database: PhDatabase,
  user: PhUser,
  keyword: PhBracketsCurly,
  phrase: PhBracketsCurly,
  privilege: PhBracketsCurly,
};

function iconFor(kind: string): Component {
  return iconMap[kind] ?? PhCube;
}
</script>

<template>
  <StateView
    v-if="items.length === 0"
    state="empty"
    title="No candidates here"
    detail="The parser found no editor-facing completion at this cursor position."
  />
  <div v-else class="candidate-groups">
    <section v-for="group in groups" :key="group.name" class="candidate-group">
      <header class="data-heading">
        <span>{{ group.name }}</span>
        <b>{{ group.items.length }}</b>
      </header>
      <div class="candidate-list">
        <button
          v-for="item in group.items"
          :key="`${item.origin}:${item.label}:${item.insertText}`"
          class="candidate-row"
          type="button"
          :aria-label="`Insert ${item.label}`"
          @click="$emit('apply', item)"
        >
          <span class="candidate-icon" :data-kind="item.kind">
            <component :is="iconFor(item.kind)" :size="15" />
          </span>
          <span class="candidate-copy">
            <strong>{{ item.label }}</strong>
            <small>{{ item.detail }}</small>
          </span>
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.candidate-group {
  border-bottom: 1px solid var(--border);
}

.data-heading {
  display: flex;
  align-items: center;
  gap: 7px;
  min-height: 38px;
  padding: 10px 16px 8px;
  color: var(--text-muted);
  font-size: 10px;
  font-weight: 720;
  letter-spacing: 0.01em;
}

.data-heading b {
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 9px;
  font-weight: 550;
}

.candidate-list {
  padding: 0 9px 9px;
}

.candidate-row {
  display: grid;
  width: 100%;
  grid-template-columns: 32px minmax(0, 1fr);
  align-items: center;
  gap: 11px;
  padding: 9px 8px;
  border: 1px solid transparent;
  border-radius: 9px;
  background: transparent;
  text-align: left;
  transition: background 120ms ease, border-color 120ms ease, transform 100ms ease;
}

.candidate-row:hover {
  border-color: color-mix(in srgb, var(--accent) 32%, var(--border));
  background: var(--surface-active);
}

.candidate-row:active {
  transform: translateY(1px);
}

.candidate-icon {
  display: grid;
  width: 32px;
  height: 32px;
  place-items: center;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-muted);
  color: var(--accent-strong);
}

.candidate-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.candidate-copy strong {
  overflow: hidden;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 650;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.candidate-copy small {
  overflow: hidden;
  color: var(--text-dim);
  font-size: 10px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
