import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TaskDto } from '@/api/client';
import RichTextEditor from '@/components/RichTextEditor.vue';
import AppButton from '@/components/ui/AppButton.vue';
import EditTaskDialog from '@/features/tasks/EditTaskDialog.vue';
import { buildI18n } from '@/i18n';
import { useTasksStore } from '@/stores/tasks';

import { ALICE, TEAM_ID } from './helpers';

const task: TaskDto = {
  id: 't1',
  status: 'active',
  position: 'a0',
  contentHtml: '<p>Old</p>',
  author: { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
  mentions: [],
  meetingId: null,
  createdAt: '2026-11-14T10:00:00Z',
};

describe('EditTaskDialog', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('locks the editor while saving, so nothing is added to the text being saved', async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useTasksStore();
    let answer: () => void = () => {};
    const update = vi.spyOn(store, 'update').mockReturnValue(
      new Promise<void>((resolve) => {
        answer = resolve;
      }),
    );
    const wrapper = mount(EditTaskDialog, {
      global: { plugins: [buildI18n(), pinia] },
      props: { open: true, task, members: [], teamId: TEAM_ID },
      attachTo: document.body,
    });
    await flushPromises();
    const editor = wrapper.findComponent(RichTextEditor);
    expect(editor.props('readonly')).toBe(false);
    editor.vm.$emit('update:modelValue', '<p>New</p>');
    await flushPromises();

    const save = wrapper.findAllComponents(AppButton).find((b) => b.text() === 'Save')!;
    await save.trigger('click');
    await flushPromises();
    expect(update).toHaveBeenCalledWith('t1', '<p>New</p>');
    expect(wrapper.findComponent(RichTextEditor).props('readonly')).toBe(true);

    answer();
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(wrapper.findComponent(RichTextEditor).props('readonly')).toBe(false);
    wrapper.unmount();
  });
});
