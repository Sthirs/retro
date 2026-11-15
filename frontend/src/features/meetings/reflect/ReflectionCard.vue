<script setup lang="ts">
/** One of my sent reflections (Reflect artboard): its text and Edit. */
import { useI18n } from 'vue-i18n';

import type { ReflectionDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import RichText from '@/components/RichText.vue';
import { useReflectDraftsStore } from '@/stores/reflectDrafts';

const props = defineProps<{ reflection: ReflectionDto }>();
const { t } = useI18n();
const drafts = useReflectDraftsStore();
</script>

<template>
  <article class="card" data-testid="my-reflection">
    <RichText class="card__text" :html="reflection.content" />
    <div class="card__foot">
      <span class="card__meta">{{ t('reflect.sent') }}</span>
      <button
        type="button"
        class="card__edit"
        :aria-label="t('reflect.edit')"
        @click="drafts.startEdit(props.reflection.id, props.reflection.column)"
      >
        <AppIcon name="pencil" :size="16" />
      </button>
    </div>
  </article>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--shadow-card);
}
.card__text {
  color: var(--text);
  font-size: 14px;
  line-height: 1.5;
}
.card__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.card__meta {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.card__edit {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 36px;
  min-height: 36px;
  position: relative;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.card__edit::before {
  /* 44 px touch target around the 36 px control. */
  content: '';
  position: absolute;
  inset: -4px;
}
.card__edit:hover {
  color: var(--text);
}
</style>
