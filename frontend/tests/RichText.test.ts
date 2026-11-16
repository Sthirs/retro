// DOMPurify reads node names through Node.prototype getters that happy-dom leaves empty,
// so it strips every tag there; jsdom implements them and is what DOMPurify is tested on.
// @vitest-environment jsdom

import { flushPromises, mount } from '@vue/test-utils';
import DOMPurify from 'dompurify';
import type { Editor } from '@tiptap/vue-3';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import { ApiError, type PersonDto } from '@/api/client';
import RichText from '@/components/RichText.vue';
import RichTextEditor from '@/components/RichTextEditor.vue';
import { buildI18n } from '@/i18n';
import { uploadAttachment } from '@/lib/attachments';
import { loadImage } from '@/lib/images';
import { sanitizeRichText } from '@/lib/sanitize';

import { ALICE, BOB } from './helpers';

vi.mock('@/lib/images', () => ({
  loadImage: vi.fn(async (url: string) => `blob:test${url}`),
}));
vi.mock('@/lib/attachments', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/attachments')>()),
  uploadAttachment: vi.fn(),
}));

const IMG_ID = '0193a4b2-7c1d-7e3f-8a9b-0c1d2e3f4a5b';
const TEAM = '0193a4b2-0000-7000-8000-000000000001';

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
      global: { plugins: [buildI18n()] },
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
      global: { plugins: [buildI18n()] },
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
  it('RichText mounts without registering sanitizer hooks', () => {
    const addHook = vi.spyOn(DOMPurify, 'addHook');
    for (let i = 0; i < 3; i++) {
      mount(RichText, {
        global: { plugins: [buildI18n()] },
        props: { html: '<p><a href="https://x.y">x</a></p>' },
      }).unmount();
    }
    expect(addHook).not.toHaveBeenCalled();
  });

  it('sanitizeRichText registers its hook once and keeps output stable', async () => {
    vi.resetModules();
    const { default: fresh } = await import('dompurify');
    const addHook = vi.spyOn(fresh, 'addHook');
    const { sanitizeRichText } = await import('@/lib/sanitize');
    const html = '<p><a href="https://x.y" rel="nofollow">x</a></p>';
    const first = sanitizeRichText(html);
    for (let i = 0; i < 3; i++) expect(sanitizeRichText(html)).toBe(first);
    expect(first).toBe('<p><a href="https://x.y" rel="noopener noreferrer">x</a></p>');
    expect(addHook).toHaveBeenCalledTimes(1);
  });

  it('RichText drops links outside http, https and mailto', () => {
    const wrapper = mount(RichText, {
      global: { plugins: [buildI18n()] },
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
      global: { plugins: [buildI18n()] },
      props: {
        html: `<p data-foo="1"><span data-type="mention" data-id="${ALICE.id}" data-label="A" data-x="2">@A</span></p>`,
      },
    });
    expect(wrapper.html()).not.toContain('data-foo');
    expect(wrapper.html()).not.toContain('data-x');
    expect(wrapper.get('span').attributes('data-id')).toBe(ALICE.id);
  });

  it.each([
    ['leading whitespace', '<p><a href=" javascript:steal()">x</a></p>'],
    ['mixed case', '<p><a href="JaVaScRiPt:steal()">x</a></p>'],
    ['entity-encoded colon', '<p><a href="javascript&colon;steal()">x</a></p>'],
    ['control characters', '<p><a href="java&#x09;script:steal()">x</a></p>'],
    ['data URI', '<p><a href="data:text/html,<script>steal()</script>">x</a></p>'],
    ['svg link', '<svg><a xlink:href="javascript:steal()"><text>x</text></a></svg>'],
    ['math mXSS', '<math><mtext><table><mglyph><style><img src=x onerror=steal()>'],
    ['noscript mXSS', '<noscript><p title="</noscript><img src=x onerror=steal()>">'],
  ])('RichText neutralises %s', (_name, html) => {
    const wrapper = mount(RichText, {
      global: { plugins: [buildI18n()] },
      props: { html },
    });
    const out = wrapper.html();
    for (const a of wrapper.findAll('a')) {
      const href = a.attributes('href');
      if (href !== undefined) expect(href).toMatch(/^(?:https?|mailto):/i);
    }
    expect(out).not.toMatch(/<(?:svg|math|img|style|script|noscript)\b/i);
    expect(out).not.toMatch(/\son\w+=/i);
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

  it('RichTextEditor locks formatting and links while readonly', async () => {
    const wrapper = mountEditor('<p>x</p>');
    const editor = await editorOf(wrapper);
    editor.commands.selectAll();
    await wrapper.get('button[aria-label="Link"]').trigger('click');
    await wrapper.get('input[type="text"]').setValue('https://example.com');
    await wrapper.setProps({ readonly: true });

    for (const label of ['Bold', 'Italic', 'Link']) {
      expect(wrapper.get(`button[aria-label="${label}"]`).attributes('disabled')).toBeDefined();
    }
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined();
    await wrapper.get('form').trigger('submit');
    expect(editor.getHTML()).toBe('<p>x</p>');

    await wrapper.setProps({ readonly: false });
    expect(wrapper.get('button[aria-label="Bold"]').attributes('disabled')).toBeUndefined();
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
    const onKey = (e: KeyboardEvent) => parentKeys.push(e.key);
    document.body.addEventListener('keydown', onKey);
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
    document.body.removeEventListener('keydown', onKey);
    wrapper.unmount();
  });
  it('RichTextEditor autolinks bare domains and emails but not other schemes', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.focus();
    editor.commands.insertContent('example.com ');
    expect(editor.getHTML()).toContain(
      '<a rel="noopener noreferrer" href="http://example.com">example.com</a>',
    );
    editor.commands.setContent('');
    editor.commands.insertContent('a@b.co ');
    expect(editor.getHTML()).toContain('href="mailto:a@b.co"');
    editor.commands.setContent('');
    editor.commands.insertContent('ftp://x.y ');
    expect(editor.getHTML()).not.toContain('<a ');
    editor.commands.setContent('');
    editor.commands.insertContent('10.0.0.1 ');
    expect(editor.getHTML()).not.toContain('<a ');
    editor.commands.setContent('');
    editor.commands.insertContent('localhost ');
    expect(editor.getHTML()).not.toContain('<a ');
    wrapper.unmount();
  });
  it.each([
    ['tab', '<a href="java&#9;script:alert(1)">x</a>'],
    ['newline', '<a href="java\nscript:alert(1)">x</a>'],
    ['control character', '<a href="\u0001javascript:alert(1)">x</a>'],
    ['entity colon', '<a href="javascript&colon;alert(1)">x</a>'],
  ])('RichTextEditor drops disguised javascript links (%s)', async (_name, anchor) => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.setContent(`<p>${anchor}</p>`);
    expect(editor.getHTML()).not.toContain('<a ');
    expect(editor.getHTML()).not.toContain('javascript');
    wrapper.unmount();
  });

  it('RichTextEditor link form rejects a disguised javascript scheme', async () => {
    const wrapper = mountEditor('<p>site</p>');
    const editor = await editorOf(wrapper);
    editor.commands.selectAll();
    await wrapper.get('button[aria-label="Link"]').trigger('click');
    await wrapper.get('form input').setValue('java\tscript:alert(1)');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(wrapper.get('form input').attributes('aria-invalid')).toBe('true');
    expect(editor.getHTML()).toBe('<p>site</p>');
    wrapper.unmount();
  });
});

describe('rich text images', () => {
  afterEach(() => {
    vi.mocked(uploadAttachment).mockReset();
  });

  it('sanitizeRichText keeps attachment images without any source', () => {
    const out = sanitizeRichText(
      `<p><img data-attachment-id="${IMG_ID}" alt="chart" width="640" height="480"` +
        ` src="https://evil.example/t.gif" srcset="x 1x" onerror="alert(1)" style="x"></p>`,
    );
    expect(out).toBe(
      `<p><img data-attachment-id="${IMG_ID}" alt="chart" width="640" height="480"></p>`,
    );
  });

  it.each([
    ['no id', '<img src="x" onerror="alert(1)">'],
    ['javascript src', '<img src="javascript:alert(1)">'],
    ['data src', '<img src="data:image/svg+xml,<svg onload=alert(1)>">'],
    ['bad id', '<img data-attachment-id="../../me">'],
    ['upper-case id', `<img data-attachment-id="${IMG_ID.toUpperCase()}">`],
  ])('sanitizeRichText removes images that are not attachments (%s)', (_name, img) => {
    expect(sanitizeRichText(`<p>a${img}</p>`)).toBe('<p>a</p>');
  });

  it('sanitizeRichText drops non-numeric sizes', () => {
    const out = sanitizeRichText(
      `<p><img data-attachment-id="${IMG_ID}" width="100%" height="1e9"></p>`,
    );
    expect(out).toBe(`<p><img data-attachment-id="${IMG_ID}"></p>`);
  });

  it('RichText loads images with the access token and shows them as blob URLs', async () => {
    const wrapper = mount(RichText, {
      global: { plugins: [buildI18n()] },
      props: { html: `<p><img data-attachment-id="${IMG_ID}" alt=""></p>` },
    });
    await flushPromises();
    const img = wrapper.get('img');
    expect(loadImage).toHaveBeenCalledWith(`/api/v1/attachments/${IMG_ID}`);
    expect(img.attributes('src')).toBe(`blob:test/api/v1/attachments/${IMG_ID}`);
    expect(img.attributes('alt')).toBe('Image');
  });

  function mountWithImages(modelValue = '') {
    return mount(RichTextEditor, {
      props: { modelValue, members: MEMBERS, label: 'Description', teamId: TEAM },
      global: { plugins: [buildI18n()] },
      attachTo: document.body,
    });
  }

  it('RichTextEditor keeps attachment images and drops images from elsewhere', async () => {
    const wrapper = mountWithImages();
    const editor = await editorOf(wrapper);
    editor.commands.setContent(
      `<p>a<img src="https://evil.example/pixel.gif" alt="x">` +
        `<img data-attachment-id="${IMG_ID}" alt="chart" width="4" height="3" src="https://evil.example/y.gif"></p>`,
    );
    expect(editor.getHTML()).toBe(
      `<p>a<img data-attachment-id="${IMG_ID}" alt="chart" width="4" height="3"></p>`,
    );
    wrapper.unmount();
  });

  it('RichTextEditor without a team drops every image and shows no image button', async () => {
    const wrapper = mountEditor();
    const editor = await editorOf(wrapper);
    editor.commands.setContent(`<p>a<img data-attachment-id="${IMG_ID}"></p>`);
    expect(editor.getHTML()).toBe('<p>a</p>');
    expect(wrapper.find('button[aria-label="Add image"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it('RichTextEditor uploads a pasted image and inserts it', async () => {
    let finish: (v: { id: string; width: number; height: number }) => void = () => {};
    vi.mocked(uploadAttachment).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const wrapper = mountWithImages('<p>see</p>');
    const editor = await editorOf(wrapper);
    editor.commands.focus('end');
    const file = new File([new Uint8Array([1])], 'shot.png', { type: 'image/png' });
    const event = { clipboardData: { files: [file] }, preventDefault: vi.fn() };
    const handled = editor.view.someProp('handlePaste', (f) =>
      f(editor.view, event as unknown as ClipboardEvent, editor.state.doc.slice(0)),
    );
    expect(handled).toBe(true);
    expect(event.preventDefault).toHaveBeenCalled();
    expect(uploadAttachment).toHaveBeenCalledWith(TEAM, file);
    await nextTick();
    expect(wrapper.emitted('uploading')?.at(-1)).toEqual([true]);
    expect(wrapper.text()).toContain('Uploading image');

    finish({ id: IMG_ID, width: 4, height: 3 });
    await flushPromises();
    expect(editor.getHTML()).toBe(
      `<p>see<img data-attachment-id="${IMG_ID}" alt="" width="4" height="3"></p>`,
    );
    expect(wrapper.emitted('uploading')?.at(-1)).toEqual([false]);
    wrapper.unmount();
  });

  it('RichTextEditor leaves pastes without images to the editor', async () => {
    const wrapper = mountWithImages();
    const editor = await editorOf(wrapper);
    const pdf = new File([new Uint8Array([1])], 'a.pdf', { type: 'application/pdf' });
    const handled = editor.view.someProp('handlePaste', (f) =>
      f(
        editor.view,
        { clipboardData: { files: [pdf] }, preventDefault: vi.fn() } as unknown as ClipboardEvent,
        editor.state.doc.slice(0),
      ),
    );
    expect(handled).toBeFalsy();
    expect(uploadAttachment).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('RichTextEditor uploads picked files and explains a refused one', async () => {
    vi.mocked(uploadAttachment).mockRejectedValue(new ApiError(415, 'unsupported_image_type'));
    const wrapper = mountWithImages('<p>x</p>');
    await editorOf(wrapper);
    const input = wrapper.get('input[type="file"]');
    expect(input.attributes('accept')).toBe('image/png,image/jpeg,image/gif,image/webp');
    const file = new File(['<svg/>'], 'a.svg', { type: 'image/svg+xml' });
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true });
    await input.trigger('change');
    await flushPromises();
    expect(uploadAttachment).toHaveBeenCalledWith(TEAM, file);
    expect(wrapper.get('[role="alert"]').text()).toBe('Use a PNG, JPEG, GIF or WebP image.');
    wrapper.unmount();
  });

  it('RichTextEditor refuses more than 10 images', async () => {
    const images = Array.from(
      { length: 10 },
      (_, i) => `<img data-attachment-id="${IMG_ID.slice(0, -1)}${i}">`,
    ).join('');
    const wrapper = mountWithImages(`<p>${images}</p>`);
    const editor = await editorOf(wrapper);
    const file = new File([new Uint8Array([1])], 'a.gif', { type: 'image/gif' });
    editor.view.someProp('handlePaste', (f) =>
      f(
        editor.view,
        { clipboardData: { files: [file] }, preventDefault: vi.fn() } as unknown as ClipboardEvent,
        editor.state.doc.slice(0),
      ),
    );
    await flushPromises();
    expect(uploadAttachment).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toBe('At most 10 images per text.');
    wrapper.unmount();
  });

  it('RichTextEditor counts queued uploads against the 10-image limit', async () => {
    vi.mocked(uploadAttachment).mockReturnValue(new Promise(() => {}));
    const wrapper = mountWithImages('<p>x</p>');
    const editor = await editorOf(wrapper);
    const paste = (count: number) =>
      editor.view.someProp('handlePaste', (f) =>
        f(
          editor.view,
          {
            clipboardData: {
              files: Array.from(
                { length: count },
                (_, i) => new File([new Uint8Array([i])], `${i}.png`, { type: 'image/png' }),
              ),
            },
            preventDefault: vi.fn(),
          } as unknown as ClipboardEvent,
          editor.state.doc.slice(0),
        ),
      );
    paste(8);
    await flushPromises();
    // Only the first of the 8 is uploading; the other 7 wait.
    expect(uploadAttachment).toHaveBeenCalledTimes(1);
    paste(5);
    await flushPromises();
    expect(uploadAttachment).toHaveBeenCalledTimes(1);
    expect(wrapper.get('[role="alert"]').text()).toBe('At most 10 images per text.');
    paste(2);
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it('RichTextEditor inserts a pasted image where it was pasted, whatever is edited meanwhile', async () => {
    let finish: (v: { id: string; width: number; height: number }) => void = () => {};
    vi.mocked(uploadAttachment).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const wrapper = mountWithImages('<p>alpha beta</p>');
    const editor = await editorOf(wrapper);
    editor.commands.setTextSelection(6); // after "alpha"
    const file = new File([new Uint8Array([1])], 'shot.png', { type: 'image/png' });
    editor.view.someProp('handlePaste', (f) =>
      f(
        editor.view,
        { clipboardData: { files: [file] }, preventDefault: vi.fn() } as unknown as ClipboardEvent,
        editor.state.doc.slice(0),
      ),
    );
    // While it uploads: text is typed before it, then "beta" is selected.
    editor.commands.insertContentAt(1, 'new ');
    editor.commands.setTextSelection({ from: 11, to: 15 });
    expect(editor.state.doc.textBetween(11, 15)).toBe('beta');

    finish({ id: IMG_ID, width: 4, height: 3 });
    await flushPromises();
    expect(editor.getHTML()).toBe(
      `<p>new alpha<img data-attachment-id="${IMG_ID}" alt="" width="4" height="3"> beta</p>`,
    );
    wrapper.unmount();
  });

  it('RichTextEditor inserts dropped images in order where they were dropped', async () => {
    const pending: ((v: { id: string; width: number; height: number }) => void)[] = [];
    vi.mocked(uploadAttachment).mockImplementation(
      () => new Promise((resolve) => pending.push(resolve)),
    );
    const wrapper = mountWithImages('<p>alpha beta</p>');
    const editor = await editorOf(wrapper);
    editor.view.posAtCoords = () => ({ pos: 6, inside: -1 }); // after "alpha"
    const files = ['a.png', 'b.png'].map(
      (name) => new File([new Uint8Array([1])], name, { type: 'image/png' }),
    );
    const handled = editor.view.someProp('handleDrop', (f) =>
      f(
        editor.view,
        {
          dataTransfer: { files },
          clientX: 0,
          clientY: 0,
          preventDefault: vi.fn(),
        } as unknown as DragEvent,
        editor.state.doc.slice(0),
        false,
      ),
    );
    expect(handled).toBe(true);
    editor.commands.insertContentAt(1, 'new ');
    const second = `${IMG_ID.slice(0, -1)}2`;
    pending[0]!({ id: IMG_ID, width: 4, height: 3 });
    await flushPromises();
    editor.commands.insertContentAt(1, 'more ');
    pending[1]!({ id: second, width: 4, height: 3 });
    await flushPromises();
    expect(editor.getHTML()).toBe(
      `<p>more new alpha<img data-attachment-id="${IMG_ID}" alt="" width="4" height="3">` +
        `<img data-attachment-id="${second}" alt="" width="4" height="3"> beta</p>`,
    );
    wrapper.unmount();
  });

  function pasteInto(editor: Editor, files: File[]): void {
    editor.view.someProp('handlePaste', (f) =>
      f(
        editor.view,
        { clipboardData: { files }, preventDefault: vi.fn() } as unknown as ClipboardEvent,
        editor.state.doc.slice(0),
      ),
    );
  }

  it('RichTextEditor keeps the selected text when the image replacing it is refused', async () => {
    let refuse: (e: unknown) => void = () => {};
    vi.mocked(uploadAttachment).mockReturnValue(
      new Promise((_, reject) => {
        refuse = reject;
      }),
    );
    const wrapper = mountWithImages('<p>alpha beta</p>');
    const editor = await editorOf(wrapper);
    editor.commands.setTextSelection({ from: 7, to: 11 }); // "beta"
    pasteInto(editor, [new File([new Uint8Array([1])], 'big.png', { type: 'image/png' })]);
    expect(editor.getHTML()).toBe('<p>alpha beta</p>');

    refuse(new ApiError(413, 'image_too_large'));
    await flushPromises();
    expect(editor.getHTML()).toBe('<p>alpha beta</p>');
    expect(wrapper.find('[role="alert"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it('RichTextEditor replaces the selected text with the images once they upload', async () => {
    const pending: ((v: { id: string; width: number; height: number }) => void)[] = [];
    vi.mocked(uploadAttachment).mockImplementation(
      () => new Promise((resolve) => pending.push(resolve)),
    );
    const wrapper = mountWithImages('<p>alpha beta</p>');
    const editor = await editorOf(wrapper);
    editor.commands.setTextSelection({ from: 7, to: 11 }); // "beta"
    const files = ['a.png', 'b.png'].map(
      (name) => new File([new Uint8Array([1])], name, { type: 'image/png' }),
    );
    pasteInto(editor, files);
    // Typed before the selection while it uploads.
    editor.commands.insertContentAt(1, 'new ');

    const second = `${IMG_ID.slice(0, -1)}2`;
    pending[0]!({ id: IMG_ID, width: 4, height: 3 });
    await flushPromises();
    pending[1]!({ id: second, width: 4, height: 3 });
    await flushPromises();
    expect(editor.getHTML()).toBe(
      `<p>new alpha <img data-attachment-id="${IMG_ID}" alt="" width="4" height="3">` +
        `<img data-attachment-id="${second}" alt="" width="4" height="3"></p>`,
    );
    wrapper.unmount();
  });
});
