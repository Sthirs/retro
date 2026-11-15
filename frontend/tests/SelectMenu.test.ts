import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, describe, expect, it } from 'vitest';
import { h } from 'vue';

import SelectMenu from '@/components/ui/SelectMenu.vue';
import { buildI18n } from '@/i18n';

const OPTIONS = [
  { value: '', label: 'All' },
  { value: 'a', label: 'Àlpha', hint: 'first' },
  { value: 'b', label: 'Beta' },
  { value: 'c', label: 'Gamma' },
];

const mounted: VueWrapper[] = [];
afterEach(() => {
  for (const w of mounted.splice(0)) w.unmount();
});

function render(props: Record<string, unknown> = {}) {
  const wrapper = mount(SelectMenu, {
    attachTo: document.body,
    props: {
      options: OPTIONS,
      modelValue: '',
      label: 'Things',
      pinned: 1,
      'onUpdate:modelValue': (value: string | string[]) => wrapper.setProps({ modelValue: value }),
      ...props,
    },
    slots: {
      trigger: ({ attrs }: { attrs: Record<string, unknown> }) =>
        h('button', { type: 'button', ...attrs }, 'Open'),
    },
    global: { plugins: [buildI18n()] },
  });
  mounted.push(wrapper);
  return wrapper;
}

const trigger = (w: VueWrapper) => w.get('button[aria-haspopup="listbox"]');
const options = (w: VueWrapper) => w.findAll('[role="option"]');
const labels = (w: VueWrapper) => options(w).map((o) => o.get('.option__label').text());
const activeLabel = (w: VueWrapper) => w.get('.option--active .option__label').text();
const emitted = (w: VueWrapper) => w.emitted<[string | string[]]>('update:modelValue') ?? [];

async function openWith(w: VueWrapper, key?: string) {
  if (key) await trigger(w).trigger('keydown', { key });
  else await trigger(w).trigger('click');
  await flushPromises();
}

describe('SelectMenu', () => {
  it('opens on click with the listbox wired to the trigger', async () => {
    const w = render();
    expect(trigger(w).attributes('aria-expanded')).toBe('false');
    expect(w.find('[role="listbox"]').exists()).toBe(false);

    await openWith(w);
    const listbox = w.get('[role="listbox"]');
    expect(trigger(w).attributes('aria-expanded')).toBe('true');
    expect(trigger(w).attributes('aria-controls')).toBe(listbox.attributes('id'));
    expect(listbox.attributes('aria-label')).toBe('Things');
    expect(labels(w)).toEqual(['All', 'Àlpha', 'Beta', 'Gamma']);
    expect(w.get('.option__hint').text()).toBe('first');
    // The first pinned option is set apart by a rule.
    expect(w.findAll('.rule')).toHaveLength(1);
    expect(document.activeElement).toBe(listbox.element);
  });

  it('starts on the selected option and moves with the arrow keys, Home and End', async () => {
    const w = render({ modelValue: 'b' });
    await openWith(w, 'ArrowDown');
    expect(activeLabel(w)).toBe('Beta');

    const listbox = w.get('[role="listbox"]');
    expect(listbox.attributes('aria-activedescendant')).toBe(options(w)[2]!.attributes('id'));
    await listbox.trigger('keydown', { key: 'ArrowDown' });
    expect(activeLabel(w)).toBe('Gamma');
    await listbox.trigger('keydown', { key: 'ArrowDown' });
    expect(activeLabel(w)).toBe('All');
    await listbox.trigger('keydown', { key: 'ArrowUp' });
    expect(activeLabel(w)).toBe('Gamma');
    await listbox.trigger('keydown', { key: 'Home' });
    expect(activeLabel(w)).toBe('All');
    await listbox.trigger('keydown', { key: 'End' });
    expect(activeLabel(w)).toBe('Gamma');
  });

  it('a single choice by Enter emits it, closes and refocuses the trigger', async () => {
    const w = render();
    await openWith(w);
    const listbox = w.get('[role="listbox"]');
    await listbox.trigger('keydown', { key: 'ArrowDown' });
    await listbox.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(emitted(w).at(-1)).toEqual(['a']);
    expect(w.find('[role="listbox"]').exists()).toBe(false);
    expect(document.activeElement).toBe(trigger(w).element);
  });

  it('a multiple choice toggles values and stays open', async () => {
    const w = render({ multiple: true, modelValue: ['b'], pinned: 0 });
    await openWith(w);
    expect(w.get('[role="listbox"]').attributes('aria-multiselectable')).toBe('true');
    expect(options(w).map((o) => o.attributes('aria-selected'))).toEqual([
      'false',
      'false',
      'true',
      'false',
    ]);

    await options(w)[3]!.trigger('click');
    expect(emitted(w).at(-1)).toEqual([['b', 'c']]);
    await w.get('[role="listbox"]').trigger('keydown', { key: ' ' });
    // The active option is still Beta (the first selected): Space unticks it.
    expect(emitted(w).at(-1)).toEqual([['c']]);
    expect(w.find('[role="listbox"]').exists()).toBe(true);
  });

  it('Esc closes and refocuses the trigger', async () => {
    const w = render();
    await openWith(w);
    await w.get('[role="listbox"]').trigger('keydown', { key: 'Escape' });
    await flushPromises();
    expect(w.find('[role="listbox"]').exists()).toBe(false);
    expect(document.activeElement).toBe(trigger(w).element);
  });

  it('searches labels ignoring case and accents, keeping pinned options', async () => {
    const w = render({ searchPlaceholder: 'Search things' });
    await openWith(w);
    const search = w.get('input[aria-label="Search things"]');
    expect(document.activeElement).toBe(search.element);
    expect(search.attributes('role')).toBe('combobox');

    await search.setValue('ALP');
    expect(labels(w)).toEqual(['All', 'Àlpha']);
    await search.trigger('keydown', { key: 'ArrowDown' });
    expect(search.attributes('aria-activedescendant')).toBe(options(w)[1]!.attributes('id'));
    await search.trigger('keydown', { key: 'Enter' });
    expect(emitted(w).at(-1)).toEqual(['a']);

    await openWith(w);
    await w.get('input').setValue('zzz');
    expect(labels(w)).toEqual(['All']);
    expect(w.text()).toContain('No matches');
  });

  it('a press outside closes the list', async () => {
    const w = render();
    await openWith(w);
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await flushPromises();
    expect(w.find('[role="listbox"]').exists()).toBe(false);
  });
});
