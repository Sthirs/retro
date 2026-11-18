import { sanitizeRichText } from '@/lib/sanitize';

/**
 * The text of a reflection's rich text, on one line, for previews and menu
 * labels. Images count as `image` (their alt text, else the given word).
 */
export function plainText(html: string, image: string): string {
  const doc = new DOMParser().parseFromString(sanitizeRichText(html), 'text/html');
  for (const img of doc.querySelectorAll('img')) {
    img.replaceWith(` ${img.getAttribute('alt') || image} `);
  }
  for (const block of doc.querySelectorAll('p, li, br')) block.append(' ');
  return (doc.body.textContent ?? '').replace(/\s+/g, ' ').trim();
}

/** Longest excerpt of a card's text, in characters, `…` included. */
export const EXCERPT_CHARS = 48;

/**
 * `text` cut to at most `max` characters, `…` included, never inside an
 * emoji or another character outside the BMP.
 */
export function excerpt(text: string, max = EXCERPT_CHARS): string {
  const chars = [...text];
  return chars.length > max ? `${chars.slice(0, max - 1).join('')}…` : text;
}
