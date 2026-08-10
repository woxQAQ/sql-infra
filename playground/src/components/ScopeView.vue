<script setup lang="ts">
import { computed } from "vue";

import type { ContextDto, RelationDto } from "../types";

const props = defineProps<{
  context: ContextDto;
}>();

const relationGroups = computed(() => {
  const groups: Array<{ name: string; relations: RelationDto[] }> = [
    { name: "Local scope", relations: props.context.scope.local },
  ];
  if (props.context.scope.dmlTarget) {
    groups.push({ name: "DML target", relations: [props.context.scope.dmlTarget] });
  }
  if (props.context.scope.mergeSource) {
    groups.push({ name: "MERGE source", relations: [props.context.scope.mergeSource] });
  }
  props.context.scope.outer.forEach((relations, index) => {
    groups.push({ name: `Outer scope ${index + 1}`, relations });
  });
  return groups;
});

function relationName(relation: RelationDto): string {
  return relation.name.map((part) => part.text).join(".") || "Unnamed relation";
}

function relationDetails(relation: RelationDto): string[] {
  const details: string[] = [];
  if (relation.lateral) details.push("Lateral");
  if (relation.qualifiedOnly) details.push("Qualified only");
  if (relation.explicitColumns.length) {
    details.push(`${relation.explicitColumns.length} explicit columns`);
  }
  return details;
}
</script>

<template>
  <div class="inspector-view scope-view">
    <div v-if="context.diagnostics.length" class="diagnostics-notice">
      <strong>Completion diagnostics</strong>
      <span
        v-for="diagnostic in context.diagnostics"
        :key="`${diagnostic.kind}:${diagnostic.range.utf8.start}`"
      >
        {{ diagnostic.kind }}
      </span>
    </div>

    <section v-for="group in relationGroups" :key="group.name" class="data-block">
      <header class="data-heading">
        <span>{{ group.name }}</span>
        <b>{{ group.relations.length }}</b>
      </header>
      <div v-if="group.relations.length" class="scope-table">
        <div class="scope-table-head" aria-hidden="true">
          <span>Relation</span>
          <span>Type</span>
          <span>Details</span>
        </div>
        <article
          v-for="relation in group.relations"
          :key="`${group.name}:${relationName(relation)}:${relation.alias?.text ?? ''}`"
          class="scope-row"
          :data-unsupported="Boolean(relation.unsupported)"
        >
          <div class="scope-name-cell">
            <strong>{{ relationName(relation) }}</strong>
            <span v-if="relation.alias">AS {{ relation.alias.text }}</span>
          </div>
          <span class="scope-kind">{{ relation.kind }}</span>
          <div class="scope-detail-cell">
            <span v-for="detail in relationDetails(relation)" :key="detail">{{ detail }}</span>
          </div>
          <small v-if="relation.unsupported" class="scope-error">{{ relation.unsupported.reason }}</small>
        </article>
      </div>
      <p v-else class="inline-empty">No visible relations</p>
    </section>

    <section class="data-block">
      <header class="data-heading">
        <span>Visible CTEs</span>
        <b>{{ context.scope.ctes.length }}</b>
      </header>
      <div v-if="context.scope.ctes.length" class="scope-table">
        <div class="scope-table-head" aria-hidden="true">
          <span>Relation</span>
          <span>Type</span>
          <span>Details</span>
        </div>
        <article v-for="cte in context.scope.ctes" :key="cte.name.text" class="scope-row">
          <div class="scope-name-cell"><strong>{{ cte.name.text }}</strong></div>
          <span class="scope-kind">CTE</span>
          <div class="scope-detail-cell">
            <span>{{ cte.explicitColumns.map((part) => part.text).join(", ") || "Derived output" }}</span>
          </div>
        </article>
      </div>
      <p v-else class="inline-empty">No visible CTEs</p>
    </section>
  </div>
</template>

<style scoped>
.inspector-view {
  min-height: 0;
}

.data-block {
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

.diagnostics-notice {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 12px;
  padding: 11px 12px;
  border: 1px solid color-mix(in srgb, var(--warning) 45%, var(--border));
  border-radius: 8px;
  background: var(--warning-surface);
  color: var(--warning);
}

.diagnostics-notice strong {
  font-size: 10px;
}

.diagnostics-notice span {
  font-family: var(--font-mono);
  font-size: 9px;
}

.scope-table {
  padding: 0 10px 10px;
}

.scope-table-head,
.scope-row {
  display: grid;
  grid-template-columns: minmax(0, 1.25fr) 86px minmax(105px, 0.85fr);
  gap: 12px;
  align-items: center;
}

.scope-table-head {
  padding: 8px 7px 6px;
  color: var(--text-dim);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.03em;
  text-transform: uppercase;
}

.scope-row {
  padding: 11px 7px;
  border-bottom: 1px solid var(--border);
}

.scope-row:last-child {
  border-bottom: 0;
}

.scope-row[data-unsupported="true"] {
  color: var(--danger);
}

.scope-name-cell {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.scope-name-cell strong {
  overflow: hidden;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scope-name-cell span {
  color: var(--accent-strong);
  font-family: var(--font-mono);
  font-size: 9px;
}

.scope-kind {
  overflow: hidden;
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 9px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scope-detail-cell {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
}

.scope-detail-cell span {
  padding: 4px 6px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--surface-muted);
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 9px;
}

.scope-error {
  grid-column: 1 / -1;
  color: var(--danger);
  font-size: 9px;
}

.inline-empty {
  margin: 0;
  padding: 0 16px 16px;
  color: var(--text-dim);
  font-size: 10px;
}

@media (max-width: 480px) {
  .scope-table-head,
  .scope-row {
    grid-template-columns: minmax(0, 1fr) 66px 90px;
    gap: 7px;
  }
}
</style>
