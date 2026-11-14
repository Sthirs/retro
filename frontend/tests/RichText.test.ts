// DOMPurify reads node names through Node.prototype getters that happy-dom leaves empty,
// so it strips every tag there; jsdom implements them and is what DOMPurify is tested on.
// @vitest-environment jsdom

import { flushPromises, mount } from '@vue/test-utils';
import type { Editor } from '@tiptap/vue-3';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import type { PersonDto } from '@/api/client';
import RichText from '@/components/RichText.vue';
import RichTextEditor from '@/components/RichTextEditor.vue';
import { buildI18n } from '@/i18n';

import { ALICE, BOB } from './helpers';

const MEMBERS: PersonDto[] = [
  { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
  { id: BOB.id, displayName: BOB.displayName, pictureVersion: null },
];

function mountEditor(modelValue = '') {
  return mount(RichTextEditor, {
    props: { modelValue, members: MEMBERS, label: 'Description' },
    global: { plugins: [buildI18n()] },
    attachTo: document.body,
  });
}

async function editorOf(wrapper: ReturnType<typeof mountEditor>): Promise<Editor> {
  await flushPromises();
  const editor = (wrapper.vm as unknown as { editor: Editor | undefined }).editor;
  if (!editor) throw new Error('editor not ready');
  return editor;
}

describe('RichText', () => {
  it('RichText renders mention chips and links', () => {
    const wrapper = mount(RichText, {
      props: {
        html:
          `<p>Hi <span data-type="mention" data-id="${ALICE.id}" data-label="Alice Rossi">@Alice Rossi</span>,` +
          ` see <a href="https://example.com" rel="noopener noreferrer">this</a> <strong>now</strong></p>`,
      },
    });
    const chip = wrapper.get('span[data-type="mention"]');
    expect(chip.text()).toBe('@Alice Rossi');
    expect(chip.attributes('data-id')).toBe(ALICE.id);
    expect(chip.attributes('data-label')).toBe('Alice Rossi');
    const link = wrapper.get('a');
    expect(link.attributes('href')).toBe('https://example.com');
    expect(link.attributes('rel')).toBe('noopener noreferrer');
    expect(wrapper.find('strong').text()).toBe('now');
  });

  it('RichText drops script and handlers', () => {
    const wrapper = mount(RichText, {
      props: {
        html:
          '<p onclick="steal()">x<script>steal()</script><img src=x onerror="steal()">' +
          '<a href="javascript:steal()" onmouseover="steal()">y</a></p>',
      },
    });
    expect(wrapper.find('script').exists()).toBe(false);
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.html()).not.toContain('onclick');
    expect(wrapper.html()).not.toContain('onerror');
    expect(wrapper.html()).not.toContain('onmouseover');
    expect(wrapper.html()).not.toContain('javascript:');
    expect(wrapper.text()).toContain('x');
    expect(wrapper.text()).toContain('y');
  });
});

describe('RichText allowlist', () => {
  it('RichText drops links outside http, https and mailto', () => {
    const wrapper = mount(RichText, {
      props: {
        html: '<p><a href="ftp://x">f</a> <a href="mailto:a@b.c">m</a> <a href="tel:1">t</a></p>',
      },
    });
    const links = wrapper.findAll('a');
    expect(links[0]?.attributes('href')).toBeUndefined();
    expect(links[1]?.attributes('href')).toBe('mailto:a@b.c');
    expect(links[2]?.attributes('href')).toBeUndefined();
  });

  it('RichText drops unlisted data attributes', () => {
    const wrapper = mount(RichText, {
      props: {
        html: `<p data-foo="1"><span data-type="mention" data-id="${ALICE.id}" data-label="A" data-x="2">@A</span></p>`,
      },
    });
    expect(wrapper.html()).not.toContain('data-foo');
    expect(wrapper.html()).not.toContain('data-x');
    expect(wrapper.get('span').attributes('data-id')).toBe(ALICE.id);
  });
});

describe('RichTextEditor', () => {
  beforeAll(() => {
    // ProseMirror measures client rects when scrolling the selection into view.
    const rect = {
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      width: 0,
      height: 0,
      toJSON: () => ({}),
    } as DOMRect;
    Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect = () => rect;
    // Mention's insert command collapses the DOM selection, which jsdom may not have yet.
    Selection.prototype.collapseToEnd = () => {};
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('RichTextEditor emits HTML when typing', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.setContent('<p>Hello <strong>world</strong></p>', { emitUpdate: true });
    await nextTick();
    const emitted = wrapper.emitted('update:modelValue');
    expect(emitted?.at(-1)).toEqual(['<p>Hello <strong>world</strong></p>']);
    expect(wrapper.get('[contenteditable="true"]').attributes('aria-label')).toBe('Description');
    wrapper.unmount();
  });

  it('RichTextEditor inserts a mention from the suggestion list', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.focus();
    editor.commands.insertContent('@bo');
    await flushPromises();

    const listbox = wrapper.get('[role="listbox"]');
    const options = listbox.findAll('[role="option"]');
    expect(options).toHaveLength(1);
    expect(options[0]?.text()).toContain('Bob Bianchi');
    expect(options[0]?.attributes('aria-selected')).toBe('true');

    await options[0]?.trigger('click');
    await flushPromises();
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
    expect(editor.getHTML()).toContain(
      `<span data-type="mention" data-id="${BOB.id}" data-label="Bob Bianchi">@Bob Bianchi</span>`,
    );
    wrapper.unmount();
  });

  it('RichTextEditor navigates suggestions with the keyboard and closes on Escape', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.focus();
    editor.commands.insertContent('@');
    await flushPromises();

    const options = () => wrapper.findAll('[role="option"]');
    expect(options()).toHaveLength(2);
    const key = async (k: string) => {
      editor.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true }));
      await flushPromises();
    };
    await key('ArrowDown');
    expect(options()[1]?.attributes('aria-selected')).toBe('true');
    await key('Escape');
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);

    editor.commands.insertContent(' @');
    await flushPromises();
    await key('ArrowDown');
    await key('Enter');
    expect(editor.getHTML()).toContain(`data-id="${BOB.id}"`);
    wrapper.unmount();
  });

  it('RichTextEditor applies a link from the inline field', async () => {
    const wrapper = mountEditor('<p>site</p>');
    const editor = await editorOf(wrapper);
    editor.commands.selectAll();
    const linkButton = wrapper.get('button[aria-label="Link"]');
    expect(linkButton.attributes('aria-pressed')).toBe('false');
    await linkButton.trigger('click');
    const input = wrapper.get('input[type="url"], input[type="text"]');
    expect(wrapper.find(`label[for="${input.attributes('id')}"]`).exists()).toBe(true);
    await input.setValue('https://example.com');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(editor.getHTML()).toBe(
      '<p><a rel="noopener noreferrer" href="https://example.com">site</a></p>',
    );
    wrapper.unmount();
  });

  it('RichTextEditor toggles bold', async () => {
    const wrapper = mountEditor('<p>x</p>');
    const editor = await editorOf(wrapper);
    editor.commands.selectAll();
    await wrapper.get('button[aria-label="Bold"]').trigger('click');
    expect(editor.getHTML()).toBe('<p><strong>x</strong></p>');
    // Tiptap's Vue wrapper re-renders state-driven bindings two animation frames later.
    await vi.waitFor(() =>
      expect(wrapper.get('button[aria-label="Bold"]').attributes('aria-pressed')).toBe('true'),
    );
    wrapper.unmount();
  });
  it.each([
    ['selection', true],
    ['empty selection', false],
  ])('RichTextEditor rejects unsupported link schemes (%s)', async (_name, selected) => {
    const wrapper = mountEditor('<p>site</p>');
    const editor = await editorOf(wrapper);
    if (selected) editor.commands.selectAll();
    else editor.commands.setTextSelection(1);
    const before = editor.getHTML();
    await wrapper.get('button[aria-label="Link"]').trigger('click');
    for (const bad of ['ftp://x', 'javascript:alert(1)']) {
      const input = wrapper.get('form input');
      await input.setValue(bad);
      await wrapper.get('form').trigger('submit');
      await flushPromises();
      expect(wrapper.find('form').exists()).toBe(true);
      expect(input.attributes('aria-invalid')).toBe('true');
      const errorId = input.attributes('aria-describedby');
      expect(errorId).toBeTruthy();
      expect(wrapper.get(`#${errorId}`).text()).not.toBe('');
      expect(editor.getHTML()).toBe(before);
    }
    wrapper.unmount();
  });

  it('RichTextEditor inserts a mailto link at an empty selection', async () => {
    const wrapper = mountEditor('<p>site</p>');
    const editor = await editorOf(wrapper);
    editor.commands.setTextSelection(1);
    await wrapper.get('button[aria-label="Link"]').trigger('click');
    await wrapper.get('form input').setValue('mailto:a@b.c');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(editor.getHTML()).toContain('href="mailto:a@b.c"');
    wrapper.unmount();
  });

  it('RichTextEditor drops links with unsupported schemes from content', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.setContent('<p><a href="ftp://x">f</a> <a href="https://ok.example">o</a></p>');
    expect(editor.getHTML()).not.toContain('ftp:');
    expect(editor.getHTML()).toContain('href="https://ok.example"');
    wrapper.unmount();
  });

  it('RichTextEditor Escape closes the mention menu without reaching a parent', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    const parentKeys: string[] = [];
    document.body.addEventListener('keydown', (e) => parentKeys.push(e.key));
    editor.commands.focus();
    editor.commands.insertContent('@');
    await flushPromises();
    const dom = editor.view.dom;
    expect(dom.getAttribute('aria-expanded')).toBe('true');
    expect(dom.getAttribute('aria-autocomplete')).toBe('list');
    expect(dom.getAttribute('aria-activedescendant')).toBeTruthy();
    dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flushPromises();
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
    expect(parentKeys).not.toContain('Escape');
    expect(dom.getAttribute('aria-activedescendant')).toBeNull();
    expect(dom.getAttribute('aria-expanded')).toBeNull();
    wrapper.unmount();
  });
});
