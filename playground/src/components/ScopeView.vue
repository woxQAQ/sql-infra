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
  return relation.name.map((part) => part.text).join(".") || "Anonymous relation";
}
</script>

<template>
  <div class="context-view">
    <div v-if="context.diagnostics.length" class="diagnostics-notice">
      <strong>Completion diagnostics</strong>
      <span
        v-for="diagnostic in context.diagnostics"
        :key="`${diagnostic.kind}:${diagnostic.range.utf8.start}`"
      >
        {{ diagnostic.kind }}, UTF-8 {{ diagnostic.range.utf8.start }}..{{ diagnostic.range.utf8.end }}
      </span>
    </div>

    <section v-for="group in relationGroups" :key="group.name" class="data-block">
      <header class="data-heading">
        <span>{{ group.name }}</span>
        <b>{{ group.relations.length }}</b>
      </header>
      <div v-if="group.relations.length" class="relation-list">
        <article
          v-for="relation in group.relations"
          :key="`${group.name}:${relationName(relation)}:${relation.alias?.text ?? ''}`"
          class="relation-row"
          :data-unsupported="Boolean(relation.unsupported)"
        >
          <div class="relation-name">
            <strong>{{ relationName(relation) }}</strong>
            <span v-if="relation.alias">as {{ relation.alias.text }}</span>
          </div>
          <span class="relation-kind">{{ relation.kind }}</span>
          <div v-if="relation.lateral || relation.qualifiedOnly || relation.explicitColumns.length" class="relation-meta">
            <span v-if="relation.lateral">Lateral</span>
            <span v-if="relation.qualifiedOnly">Qualified only</span>
            <span v-if="relation.explicitColumns.length">
              {{ relation.explicitColumns.length }} explicit columns
            </span>
          </div>
          <small v-if="relation.unsupported">{{ relation.unsupported.reason }}</small>
        </article>
      </div>
      <p v-else class="inline-empty">No visible relations</p>
    </section>

    <section class="data-block">
      <header class="data-heading">
        <span>Visible CTEs</span>
        <b>{{ context.scope.ctes.length }}</b>
      </header>
      <div v-if="context.scope.ctes.length" class="relation-list">
        <article v-for="cte in context.scope.ctes" :key="cte.name.text" class="relation-row">
          <div class="relation-name"><strong>{{ cte.name.text }}</strong></div>
          <small>
            {{ cte.explicitColumns.map((part) => part.text).join(", ") || "Derived output" }}
          </small>
        </article>
      </div>
      <p v-else class="inline-empty">No visible CTEs</p>
    </section>
  </div>
</template>
