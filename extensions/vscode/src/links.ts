export type LinkTarget = { kind: 'web'; url: string } | { kind: 'document'; path: string; fragment?: string };

export function linkTarget(href: string): LinkTarget {
  if (!href || /[\u0000-\u001f\u007f]/.test(href)) throw new Error('The link is empty or contains control characters.');
  if (/^(?:https?:)?\/\//i.test(href)) {
    const url = new URL(href.startsWith('//') ? `https:${href}` : href);
    if (!['http:', 'https:'].includes(url.protocol) || !url.hostname) throw new Error('Invalid HTTP/HTTPS URL.');
    return { kind: 'web', url: url.href };
  }
  if (/^file:/i.test(href)) {
    const url = new URL(href);
    if (url.hostname && url.hostname !== 'localhost') throw new Error('Only local file URLs can be opened.');
    return { kind: 'document', path: decodePath(url.pathname), fragment: url.hash ? decodeURIComponent(url.hash.slice(1)) : undefined };
  }
  if (!/^[a-z]:[\\/]/i.test(href) && /^[a-z][a-z\d+.-]*:/i.test(href)) throw new Error('Use an HTTP/HTTPS URL or a local document path.');
  const hash = href.indexOf('#');
  const path = hash < 0 ? href : href.slice(0, hash);
  return { kind: 'document', path: decodePath(path), fragment: hash < 0 ? undefined : decodeURIComponent(href.slice(hash + 1)) };
}
function decodePath(path: string): string {
  const decoded = decodeURIComponent(path);
  if (/[\u0000-\u001f\u007f]/.test(decoded)) throw new Error('Invalid control character in document path.');
  return decoded;
}
export function headingSlug(text: string): string {
  return text.toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');
}
