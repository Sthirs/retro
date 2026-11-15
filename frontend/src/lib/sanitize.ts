import DOMPurify from 'dompurify';

/**
 * Sanitiser for stored rich text (ADR-0010): the tags, attributes and link
 * schemes the server keeps, for tasks and reflections alike (reflections add
 * strikethrough, bullet lists and inline code; tasks add mention spans). The hook below is registered once, at module load.
 */
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  // The server always sets rel on links; do the same so stored HTML can't opt out.
  if (node.tagName === 'A' && node.hasAttribute('href')) {
    node.setAttribute('rel', 'noopener noreferrer');
  }
});

export function sanitizeRichText(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ['p', 'br', 'strong', 'em', 's', 'a', 'span', 'ul', 'li', 'code'],
    ALLOWED_ATTR: ['href', 'rel', 'data-type', 'data-id', 'data-label'],
    ALLOW_DATA_ATTR: false,
    // Plain-text values, not URIs: exempt them from the scheme check below.
    ADD_URI_SAFE_ATTR: ['rel', 'data-type', 'data-id', 'data-label'],
    ALLOWED_URI_REGEXP: /^(?:https?|mailto):/i,
  });
}
