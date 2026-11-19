<script setup lang="ts">
/**
 * Someone else writing a new task for the group (Discuss artboard, spec
 * §9.5): a dashed placeholder with their name and two shimmering lines.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ParticipantDto } from '@/api/client';
import UserAvatar from '@/components/UserAvatar.vue';
import WritingDots from '@/features/meetings/discuss/WritingDots.vue';
import { firstName } from '@/features/meetings/firstName';

const props = defineProps<{ writer: ParticipantDto }>();
const { t } = useI18n();
const label = computed(() =>
  t('discuss.tasks.writing', { name: firstName(props.writer.displayName) }),
);
</script>

<template>
  <li class="writing" role="status" :aria-label="label" data-testid="discuss-task-writing">
    <span class="writing__who">
      <UserAvatar
        :user-id="writer.id"
        :name="writer.displayName"
        :picture-version="writer.pictureVersion"
        :size="20"
        ring="var(--well)"
      />
      {{ label }}
      <WritingDots />
    </span>
    <span class="writing__line" aria-hidden="true" />
    <span class="writing__line writing__line--short" aria-hidden="true" />
  </li>
</template>

<style scoped>
.writing {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px;
  border: 1px dashed var(--line);
  border-radius: var(--radius-control);
}
.writing__who {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 600;
}
.writing__line {
  display: block;
  width: 92%;
  height: 10px;
  border-radius: var(--radius-pill);
  background: linear-gradient(90deg, var(--well) 0%, var(--accent-soft) 50%, var(--well) 100%);
  background-size: 200% 100%;
  animation: retroWriting 1.6s linear infinite;
}
.writing__line--short {
  width: 64%;
}
@media (prefers-reduced-motion: reduce) {
  .writing__line {
    animation: none;
  }
}
</style>
