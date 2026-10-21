import { $view } from '@milkdown/kit/utils';
import { codeBlockSchema, imageSchema } from '@milkdown/kit/preset/commonmark';
import { extendListItemSchemaForTask } from '@milkdown/kit/preset/gfm';
import type { Node } from '@milkdown/kit/prose/model';
import { Selection } from '@milkdown/kit/prose/state';
import { DiagramZoom } from './diagram-zoom';
let diagramId = 0;
let engine: Promise<typeof import('mermaid')['default']> | undefined;
function mermaid() {
  return engine ??= import('mermaid').then(({ default: m }) => {
    m.initialize({ startOnLoad: false, securityLevel: 'strict', theme: document.body.classList.contains('vscode-light') ? 'default' : 'dark', suppressErrorRendering: true, maxTextSize: 50_000, flowchart: { htmlLabels: false } });
    return m;
  });
}

export const codeView = $view(codeBlockSchema.node, () => (initial, view, getPos) => {
  let node = initial, generation = 0, destroyed = false;
  let timer: ReturnType<typeof setTimeout>;
  const dom = document.createElement('div'); dom.className = 'code-block';
  const controls = document.createElement('div'); controls.className = 'code-controls'; controls.contentEditable = 'false';
  const label = document.createElement('span');
  const toggle = document.createElement('button'); toggle.type = 'button'; toggle.textContent = 'Edit diagram';
  const pre = document.createElement('pre');
  // Use the block itself as the editable root. Chromium can remove a nested
  // <code> wrapper when replacing all its text, leaving a detached contentDOM.
  const contentDOM = pre;
  contentDOM.spellcheck = false;
  const preview = document.createElement('div'); preview.className = 'diagram-preview'; preview.contentEditable = 'false';
  const zoom = new DiagramZoom(preview);
  const error = document.createElement('p'); error.className = 'diagram-error'; error.contentEditable = 'false'; error.hidden = true;
  controls.append(label, zoom.controls, toggle); dom.append(controls, pre, preview, error);
  let editing = false, previousSource = '';
  const setEditing = (value: boolean) => {
    editing = value;
    pre.hidden = node.attrs.language === 'mermaid' && !editing;
    toggle.textContent = editing ? 'Hide code' : 'Edit diagram';
    if (editing) { view.focus(); const pos = getPos(); if (typeof pos === 'number') view.dispatch(view.state.tr.setSelection(Selection.near(view.state.doc.resolve(pos + 1)))); }
  };
  toggle.addEventListener('click', () => setEditing(!editing));
  preview.addEventListener('dblclick', () => setEditing(true));
  async function render(source: string, id: number) {
    try {
      if (!source.trim()) { preview.textContent = 'Write a Mermaid diagram to preview it.'; zoom.rendered(); error.hidden = true; return; }
      const m = await mermaid();
      if (destroyed || id !== generation) return;
      const { svg } = await m.render(`tpreview-diagram-${++diagramId}`, source);
      if (destroyed || id !== generation) return;
      // Mermaid's strict mode sanitizes generated SVG; document HTML is never inserted.
      preview.innerHTML = svg;
      zoom.rendered();
      error.hidden = true;
    } catch {
      if (destroyed || id !== generation) return;
      error.textContent = preview.querySelector('svg') ? 'Incomplete Mermaid syntax. Showing the last valid diagram.' : 'Unable to render this diagram. Choose Edit diagram to check the Mermaid syntax.';
      error.hidden = false;
    }
  }
  function refresh() {
    const isDiagram = node.attrs.language === 'mermaid';
    label.textContent = node.attrs.language || 'Code';
    toggle.hidden = !isDiagram; preview.hidden = !isDiagram; zoom.controls.hidden = !isDiagram;
    pre.hidden = isDiagram && !editing;
    if (isDiagram && (node.textContent !== previousSource || !generation)) {
      previousSource = node.textContent;
      const id = ++generation;
      clearTimeout(timer); timer = setTimeout(() => void render(node.textContent, id), 180);
    }
    if (!isDiagram) { ++generation; clearTimeout(timer); error.hidden = true; }
  }
  refresh();
  return {
    dom, contentDOM,
    update(next: Node) { if (next.type !== node.type) return false; node = next; refresh(); return true; },
    stopEvent(event: Event) { return controls.contains(event.target as globalThis.Node) || preview.contains(event.target as globalThis.Node); },
    ignoreMutation(mutation: { type: string; target: globalThis.Node }) {
      if (mutation.type === 'attributes' && mutation.target === pre) return true;
      return mutation.type !== 'selection' && !contentDOM.contains(mutation.target);
    },
    destroy() { destroyed = true; ++generation; clearTimeout(timer); zoom.destroy(); },
  };
});

export const taskView = $view(extendListItemSchemaForTask.node, () => (initial, view, getPos) => {
  let node = initial;
  const dom = document.createElement('li');
  const checkbox = document.createElement('input'); checkbox.type = 'checkbox'; checkbox.contentEditable = 'false'; checkbox.setAttribute('aria-label', 'Task complete');
  const contentDOM = document.createElement('div'); contentDOM.className = 'list-content';
  dom.append(checkbox, contentDOM);
  const refresh = () => { checkbox.hidden = node.attrs.checked == null; checkbox.checked = !!node.attrs.checked; dom.classList.toggle('task-item', node.attrs.checked != null); };
  refresh();
  checkbox.addEventListener('change', () => {
    const pos = getPos();
    if (typeof pos === 'number' && view.editable) view.dispatch(view.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, checked: checkbox.checked }));
  });
  return { dom, contentDOM, update(next: Node) { if (next.type !== node.type) return false; node = next; refresh(); return true; }, stopEvent(event: Event) { return event.target === checkbox; }, ignoreMutation(m: { type: string; target: globalThis.Node }) { return m.type !== 'selection' && !contentDOM.contains(m.target); } };
});

export function imageView(baseUri: () => string) {
  return $view(imageSchema.node, () => node => {
    const dom = document.createElement('span'); dom.className = 'image-wrapper';
    const img = document.createElement('img'); img.alt = node.attrs.alt || 'Image'; img.title = node.attrs.title || '';
    const src = String(node.attrs.src ?? '');
    // External images are not fetched automatically. Local images are scoped by VS Code.
    if (!/^[a-z][a-z\d+.-]*:|^\/\//i.test(src)) {
      try { img.src = new URL(src, baseUri()).href; dom.append(img); } catch { dom.textContent = `[Image: ${img.alt}]`; }
      img.addEventListener('error', () => { dom.textContent = `[Image unavailable: ${img.alt}]`; });
    } else dom.textContent = `[External image: ${img.alt}]`;
    return { dom };
  });
}
