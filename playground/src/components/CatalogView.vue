<script setup lang="ts">
import { computed } from "vue";

import { buildCatalogTree } from "../catalog-tree";
import type { CatalogDocument } from "../types";
import CatalogTreeNode from "./CatalogTreeNode.vue";

const props = defineProps<{
  catalog: CatalogDocument;
}>();

const tree = computed(() => buildCatalogTree(props.catalog));
</script>

<template>
  <div class="catalog-view inspector-view">
    <div class="catalog-summary">
      <span>Search path</span>
      <div class="catalog-path">
        <code v-for="path in catalog.searchPath ?? []" :key="path">{{ path }}</code>
      </div>
    </div>

    <div v-if="tree.length" class="catalog-tree" role="tree" aria-label="Catalog objects">
      <CatalogTreeNode v-for="node in tree" :key="node.key" :node="node" />
    </div>
    <p v-else class="catalog-empty">No catalog objects</p>
  </div>
</template>

<style scoped>
.inspector-view {
  min-height: 0;
}

.catalog-summary {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 48px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border);
}

.catalog-summary > span {
  color: var(--text-dim);
  font-size: 10px;
  font-weight: 680;
}

.catalog-path {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.catalog-path code {
  padding: 4px 6px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--surface-muted);
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 9px;
}

.catalog-tree {
  padding: 8px 10px 12px;
}

.catalog-empty {
  margin: 0;
  padding: 18px 16px;
  color: var(--text-dim);
  font-size: 10px;
}
</style>
