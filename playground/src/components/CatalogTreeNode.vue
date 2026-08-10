<script setup lang="ts">
import { ref, type Component } from "vue";
import {
  PhBracketsCurly,
  PhCaretDown,
  PhCaretRight,
  PhColumns,
  PhCube,
  PhDatabase,
  PhFunction,
  PhTable,
  PhTreeStructure,
  PhUser,
} from "@phosphor-icons/vue";

import type { CatalogTreeNode as CatalogNode } from "../catalog-tree";

const props = defineProps<{
  node: CatalogNode;
}>();

const hasChildren = props.node.children.length > 0 || Boolean(props.node.members?.length);
const expanded = ref(hasChildren);

const iconMap: Record<string, Component> = {
  column: PhColumns,
  database: PhDatabase,
  function: PhFunction,
  role: PhUser,
  schema: PhTreeStructure,
  table: PhTable,
  type: PhBracketsCurly,
};

function iconFor(kind?: string): Component {
  return iconMap[kind?.toLowerCase() ?? ""] ?? PhCube;
}
</script>

<template>
  <div class="catalog-node" role="treeitem" :aria-expanded="hasChildren ? expanded : undefined">
    <button
      class="catalog-row"
      type="button"
      :disabled="!hasChildren"
      :aria-label="`${hasChildren ? 'Toggle' : 'View'} ${node.name}`"
      @click="hasChildren && (expanded = !expanded)"
    >
      <span class="catalog-caret" aria-hidden="true">
        <PhCaretDown v-if="expanded" :size="13" weight="bold" />
        <PhCaretRight v-else-if="hasChildren" :size="13" weight="bold" />
      </span>
      <span class="catalog-icon" aria-hidden="true">
        <component :is="iconFor(node.kind)" :size="15" />
      </span>
      <span class="catalog-label">
        <strong>{{ node.name }}</strong>
        <small v-if="node.detail">{{ node.detail }}</small>
      </span>
      <span v-if="node.kind" class="catalog-kind">{{ node.kind }}</span>
    </button>

    <div v-if="expanded" class="catalog-children" role="group">
      <CatalogTreeNode v-for="child in node.children" :key="child.key" :node="child" />
      <div
        v-for="member in node.members ?? []"
        :key="`${node.key}:${member.kind}:${member.name}`"
        class="catalog-row catalog-member"
      >
        <span class="catalog-indent" aria-hidden="true" />
        <span class="catalog-icon" aria-hidden="true">
          <component :is="iconFor(member.kind)" :size="14" />
        </span>
        <span class="catalog-label">
          <strong>{{ member.name }}</strong>
          <small v-if="member.detail">{{ member.detail }}</small>
        </span>
        <span class="catalog-kind">{{ member.kind }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.catalog-node {
  border-bottom: 1px solid var(--border);
}

.catalog-node:last-child {
  border-bottom: 0;
}

.catalog-row {
  display: grid;
  width: 100%;
  grid-template-columns: 16px 28px minmax(0, 1fr) auto;
  align-items: center;
  gap: 9px;
  min-height: 44px;
  padding: 6px 7px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  text-align: left;
}

button.catalog-row:not(:disabled) {
  cursor: pointer;
}

button.catalog-row:not(:disabled):hover {
  border-color: color-mix(in srgb, var(--accent) 32%, var(--border));
  background: var(--surface-active);
}

.catalog-row:disabled {
  cursor: default;
}

.catalog-caret,
.catalog-indent {
  display: grid;
  width: 16px;
  height: 16px;
  place-items: center;
  color: var(--text-dim);
}

.catalog-icon {
  display: grid;
  width: 28px;
  height: 28px;
  place-items: center;
  border: 1px solid var(--border);
  border-radius: 7px;
  background: var(--surface-muted);
  color: var(--accent-strong);
}

.catalog-label {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.catalog-label strong {
  overflow: hidden;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 650;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.catalog-label small {
  overflow: hidden;
  color: var(--text-dim);
  font-size: 9px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.catalog-kind {
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 9px;
}

.catalog-children {
  margin: 0 0 6px 32px;
  padding-left: 12px;
  border-left: 1px solid var(--border-strong);
}

.catalog-member {
  min-height: 38px;
}

.catalog-member .catalog-icon {
  width: 26px;
  height: 26px;
}
</style>
