<script setup lang="ts">
/**
 * The Reflect composer (Reflect artboard): writes a new reflection in a
 * column, or edits, moves or deletes one of mine. A new reflection's text is
 * a draft kept in this browser until it is sent (spec §9.2). Ctrl/Cmd +
 * Enter sends; Esc cancels.
 */
import { computed, ref, useId } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ReflectionColumn, ReflectionDto } from '@/api/client';
import RichTextEditor from '@/components/RichTextEditor.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { COLUMNS } from '@/features/meetings/reflect/columns';
import { useMeetingStore } from '@/stores/meeting';
import { hasText, type OpenEditor, useReflectDraftsStore } from '@/stores/reflectDrafts';

const props = defineProps<{ column: ReflectionColumn; reflection?: ReflectionDto }>();
const { t } = useI18n();
const store = useMeetingStore();
const drafts = useReflectDraftsStore();
const uid = useId();

const editing = computed(() => !!props.reflection);
const name = computed(() => t(`reflect.columns.${props.column}.name`));
const edited = ref(props.reflection?.content ?? '');
const target = ref<ReflectionColumn>(props.reflection?.column ?? props.column);
const busy = ref(false);

const content = computed({
  get: () => (editing.value ? edited.value : drafts.draft(props.column)),
  set: (html: string) => {
    if (editing.value) edited.value = html;
    else drafts.setDraft(props.column, html);
  },
});
const canSend = computed(() => hasText(content.value) && !busy.value);

/**
 * After a request, closes the editor that started it, if it is still the
 * open one: it may have been cancelled and another opened meanwhile.
 */
function closeIfStill(session: OpenEditor | null): void {
  if (drafts.open === session) drafts.close();
}

async function submit(): Promise<void> {
  if (!canSend.value) return;
  const session = drafts.open;
  busy.value = true;
  try {
    if (props.reflection) {
      const change: { content?: string; column?: ReflectionColumn } = {};
      if (edited.value !== props.reflection.content) change.content = edited.value;
      if (target.value !== props.reflection.column) change.column = target.value;
      if (
        Object.keys(change).length === 0 ||
        (await store.updateReflection(props.reflection.id, change))
      ) {
        closeIfStill(session);
      }
    } else {
      const column = props.column;
      const sent = content.value;
      if (await store.createReflection(column, sent)) {
        // Text typed while the request was on its way stays a draft.
        if (drafts.draft(column) === sent) {
          drafts.clearDraft(column);
          closeIfStill(session);
        }
      }
    }
  } finally {
    busy.value = false;
  }
}

async function remove(): Promise<void> {
  if (!props.reflection || busy.value) return;
  const session = drafts.open;
  busy.value = true;
  try {
    if (await store.deleteReflection(props.reflection.id)) closeIfStill(session);
  } finally {
    busy.value = false;
  }
}

function cancel(): void {
  if (!editing.value) drafts.clearDraft(props.column);
  drafts.close();
}
</script>

<template>
  <div class="composer">
    <RichTextEditor
      v-model="content"
      kind="reflection"
      autofocus
      :label="t('reflect.editorLabel', { column: name })"
      :placeholder="`${t(`reflect.columns.${column}.hint`)}…`"
      @submit="submit"
      @cancel="cancel"
    />
    <div v-if="editing" class="composer__edit">
      <label class="composer__label" :for="`${uid}-column`">{{ t('reflect.moveTo') }}</label>
      <select :id="`${uid}-column`" v-model="target" class="composer__select">
        <option v-for="c in COLUMNS" :key="c" :value="c">
          {{ t(`reflect.columns.${c}.name`) }}
        </option>
      </select>
      <AppButton variant="danger-outline" size="compact" :disabled="busy" @click="remove">
        {{ t('reflect.delete') }}
      </AppButton>
    </div>
    <div class="composer__foot">
      <span class="composer__hint">{{ t('reflect.sendHint') }}</span>
      <span class="composer__actions">
        <AppButton variant="ghost" size="compact" @click="cancel">
          {{ t('reflect.cancel') }}
        </AppButton>
        <AppButton variant="primary" size="compact" :disabled="!canSend" @click="submit">
          {{ editing ? t('reflect.save') : t('reflect.send') }}
        </AppButton>
      </span>
    </div>
  </div>
</template>

<style scoped>
.composer {
  display: flex;
  flex-direction: column;
  border: 2px solid var(--accent);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: 0 0 0 4px var(--accent-soft);
  overflow: hidden;
}
.composer__edit {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 8px 8px 8px 14px;
  border-top: 1px solid var(--line);
}
.composer__label {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.composer__select {
  flex: 1 1 100px;
  min-width: 0;
  min-height: 44px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  font-size: 13px;
}
.composer__foot {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px 8px 14px;
  border-top: 1px solid var(--line);
}
.composer__hint {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.composer__actions {
  display: flex;
  gap: 4px;
}
</style>
