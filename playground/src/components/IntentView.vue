<script setup lang="ts">
import { computed } from "vue";

import type { ContextDto } from "../types";

const props = defineProps<{
  context: ContextDto;
}>();

const qualifier = computed(
  () => props.context.intent.qualifier.map((part) => part.text).join(".") || "None",
);

const metrics = computed(() => [
  {
    label: "Prefix",
    value: props.context.prefix.raw || "Empty",
    detail: `${props.context.prefix.quoting}, normalized ${props.context.prefix.normalized || "empty"}`,
  },
  {
    label: "Qualifier",
    value: qualifier.value,
    detail: `${props.context.intent.qualifier.length} completed name parts`,
  },
  {
    label: "Point",
    value: `UTF-16 ${props.context.point.effectiveUtf16}`,
    detail: `UTF-8 ${props.context.point.utf8}${props.context.point.adjusted ? ", adjusted" : ""}`,
  },
  {
    label: "Replacement",
    value: `${props.context.replacementRange.utf16.start}..${props.context.replacementRange.utf16.end}`,
    detail: `UTF-8 ${props.context.replacementRange.utf8.start}..${props.context.replacementRange.utf8.end}`,
  },
]);

const groups = computed(() => [
  ["Grammar slots", props.context.expectations.slots],
  ["Object intent", props.context.intent.objectKinds],
  ["Tokens", props.context.expectations.tokens],
  ["Direct syntax", props.context.expectations.directTokens],
  ["Lookahead syntax", props.context.expectations.lookaheadTokens],
  ["Expression starts", props.context.expectations.expressionStartTokens],
  ["Expression continuations", props.context.expectations.expressionContinuationTokens],
  ["Expression follows", props.context.expectations.followTokens],
  ["Phrases", props.context.expectations.phrases],
] as const);
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

    <div class="metric-grid">
      <div v-for="metric in metrics" :key="metric.label" class="metric-item">
        <span>{{ metric.label }}</span>
        <strong>{{ metric.value }}</strong>
        <small>{{ metric.detail }}</small>
      </div>
    </div>

    <section v-if="context.intent.container" class="data-block container-block">
      <header class="data-heading"><span>Container</span></header>
      <div class="container-row">
        <strong>{{ context.intent.container.name.map((part) => part.text).join(".") }}</strong>
        <span>{{ context.intent.container.objectKinds.join(" / ") }}</span>
        <small>Members: {{ context.intent.container.members.join(", ") }}</small>
      </div>
    </section>

    <section v-for="group in groups" :key="group[0]" class="data-block">
      <header class="data-heading">
        <span>{{ group[0] }}</span>
        <b>{{ group[1].length }}</b>
      </header>
      <div v-if="group[1].length" class="token-list">
        <code v-for="value in group[1]" :key="value">{{ value }}</code>
      </div>
      <p v-else class="inline-empty">Nothing collected</p>
    </section>
  </div>
</template>
