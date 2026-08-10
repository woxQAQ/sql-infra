<script setup lang="ts">
import { computed } from "vue";

import type { ContextDto } from "../types";

const props = defineProps<{
  context: ContextDto;
}>();

const qualifier = computed(
  () => props.context.intent.qualifier.map((part) => part.text).join(".") || "None",
);

const objectKinds = computed(
  () => props.context.intent.objectKinds,
);

const expectationGroups = computed(() => [
  { label: "Grammar slots", values: props.context.expectations.slots },
  { label: "Tokens", values: props.context.expectations.tokens },
  { label: "Direct syntax", values: props.context.expectations.directTokens },
  { label: "Lookahead syntax", values: props.context.expectations.lookaheadTokens },
  { label: "Expression starts", values: props.context.expectations.expressionStartTokens },
  { label: "Expression continuations", values: props.context.expectations.expressionContinuationTokens },
  { label: "Expression follows", values: props.context.expectations.followTokens },
  { label: "Phrases", values: props.context.expectations.phrases },
].filter((group) => group.values.length));
</script>

<template>
  <div class="inspector-view intent-view">
    <div v-if="context.diagnostics.length" class="diagnostics-notice">
      <strong>Completion diagnostics</strong>
      <span
        v-for="diagnostic in context.diagnostics"
        :key="`${diagnostic.kind}:${diagnostic.range.utf8.start}`"
      >
        {{ diagnostic.kind }}
      </span>
    </div>

    <section class="intent-overview">
      <div class="intent-overview-block">
        <span>Object intent</span>
        <div v-if="objectKinds.length" class="token-list compact-token-list">
          <code v-for="kind in objectKinds" :key="kind">{{ kind }}</code>
        </div>
        <strong v-else>None detected</strong>
      </div>
      <div class="intent-overview-block">
        <span>Qualifier</span>
        <strong>{{ qualifier }}</strong>
      </div>
    </section>

    <section class="data-block">
      <header class="data-heading"><span>Identifier</span></header>
      <div class="detail-list">
        <div class="detail-row">
          <span>Prefix</span>
          <code>{{ context.prefix.raw || "Empty" }}</code>
        </div>
        <div class="detail-row">
          <span>Normalized</span>
          <code>{{ context.prefix.normalized || "Empty" }}</code>
        </div>
        <div class="detail-row">
          <span>Quoting</span>
          <code>{{ context.prefix.quoting }}</code>
        </div>
      </div>
    </section>

    <section v-if="context.intent.membership" class="data-block">
      <header class="data-heading"><span>Catalog membership</span></header>
      <div class="membership-row">
        <strong>{{ context.intent.membership.owner.name.map((part) => part.text).join(".") }}</strong>
        <span>{{ context.intent.membership.owner.objectKinds.join(" / ") }}</span>
        <small>Member kinds: {{ context.intent.membership.memberKinds.join(", ") }}</small>
      </div>
    </section>

    <section class="data-block expectation-block">
      <header class="data-heading"><span>Syntax expectations</span></header>
      <div v-if="expectationGroups.length" class="expectation-list">
        <div v-for="group in expectationGroups" :key="group.label" class="expectation-row">
          <span>{{ group.label }}</span>
          <div class="token-list compact-token-list">
            <code v-for="value in group.values" :key="value">{{ value }}</code>
          </div>
        </div>
      </div>
      <p v-else class="inline-empty">No syntax expectations</p>
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

.intent-overview {
  display: grid;
  grid-template-columns: 1.1fr 0.9fr;
  border-bottom: 1px solid var(--border);
}

.intent-overview-block {
  display: flex;
  min-width: 0;
  min-height: 104px;
  flex-direction: column;
  justify-content: center;
  gap: 9px;
  padding: 16px;
  border-right: 1px solid var(--border);
}

.intent-overview-block:last-child {
  border-right: 0;
}

.intent-overview-block > span,
.detail-row > span,
.expectation-row > span {
  color: var(--text-dim);
  font-size: 10px;
  font-weight: 680;
}

.intent-overview-block > strong {
  overflow: hidden;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.token-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.compact-token-list {
  padding: 0;
}

.token-list code {
  padding: 4px 6px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--surface-muted);
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 9px;
}

.compact-token-list code {
  color: var(--accent-strong);
  border-color: color-mix(in srgb, var(--accent) 25%, var(--border));
  background: var(--accent-soft);
}

.detail-list {
  display: grid;
  padding: 4px 16px 12px;
}

.detail-row {
  display: grid;
  grid-template-columns: 110px minmax(0, 1fr);
  align-items: center;
  gap: 12px;
  min-height: 34px;
  border-bottom: 1px solid var(--border);
}

.detail-row:last-child {
  border-bottom: 0;
}

.detail-row code {
  overflow: hidden;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 10px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.membership-row {
  display: grid;
  gap: 4px;
  margin: 0 14px 14px;
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--accent) 25%, var(--border));
  border-radius: 8px;
  background: var(--accent-soft);
}

.membership-row strong {
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 11px;
}

.membership-row > span {
  color: var(--text-muted);
  font-size: 9px;
}

.membership-row small {
  overflow: hidden;
  color: var(--text-dim);
  font-size: 9px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.expectation-list {
  padding: 0 16px 12px;
}

.expectation-row {
  display: grid;
  grid-template-columns: 145px minmax(0, 1fr);
  align-items: start;
  gap: 12px;
  padding: 9px 0;
  border-bottom: 1px solid var(--border);
}

.expectation-row:last-child {
  border-bottom: 0;
}

.inline-empty {
  margin: 0;
  padding: 0 16px 16px;
  color: var(--text-dim);
  font-size: 10px;
}

@media (max-width: 480px) {
  .intent-overview {
    grid-template-columns: 1fr;
  }

  .intent-overview-block,
  .intent-overview-block:last-child {
    border-right: 0;
    border-bottom: 1px solid var(--border);
  }

  .intent-overview-block:last-child {
    border-bottom: 0;
  }

  .expectation-row {
    grid-template-columns: 1fr;
    gap: 7px;
  }
}
</style>
