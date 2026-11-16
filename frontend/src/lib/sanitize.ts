import DOMPurify from 'dompurify';

/** A canonical (lowercase, hyphenated) UUID, as the server writes attachment ids. */
export const ATTACHMENT_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/**
 * Sanitiser for stored rich text (ADR-0010): the tags, attributes and link
 * schemes the server keeps, for tasks and reflections alike (reflections add
 * strikethrough, bullet lists and inline code; tasks add mention spans).
 * Images carry an attachment id and never a `src` (ADR-0013): `RichText`
 * loads them with the access token. The hook below is registered once, at
 * module load.
 */
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  // The server always sets rel on links; do the same so stored HTML can't opt out.
  if (node.tagName === 'A' && node.hasAttribute('href')) {
    node.setAttribute('rel', 'noopener noreferrer');
  }
  if (node.tagName === 'IMG') {
    if (!ATTACHMENT_ID.test(node.getAttribute('data-attachment-id') ?? '')) {
      node.remove();
      return;
    }
    for (const name of ['width', 'height']) {
      const value = node.getAttribute(name);
      if (value !== null && !/^[1-9]\d{0,4}$/.test(value)) node.removeAttribute(name);
    }
  }
});

export function sanitizeRichText(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ['p', 'br', 'strong', 'em', 's', 'a', 'span', 'ul', 'li', 'code', 'img'],
    ALLOWED_ATTR: [
      'href',
      'rel',
      'data-type',
      'data-id',
      'data-label',
      'data-attachment-id',
      'alt',
      'width',
      'height',
    ],
    ALLOW_DATA_ATTR: false,
    // Plain-text values, not URIs: exempt them from the scheme check below.
    ADD_URI_SAFE_ATTR: [
      'rel',
      'data-type',
      'data-id',
      'data-label',
      'data-attachment-id',
      'width',
      'height',
    ],
    ALLOWED_URI_REGEXP: /^(?:https?|mailto):/i,
  });
}
