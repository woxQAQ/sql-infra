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
          <code>{{ item.insertText }}</code>
        </button>
      </div>
    </section>
  </div>
</template>
