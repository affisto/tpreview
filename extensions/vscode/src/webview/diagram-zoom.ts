/** View-only zoom/pan: never dispatch a Markdown transaction. */
export class DiagramZoom {
  readonly controls = document.createElement('div');
  private out = this.button('−', 'Zoom out');
  private reset = this.button('100%', 'Reset diagram zoom to 100%');
  private increase = this.button('+', 'Zoom in');
  private fit = this.button('Fit', 'Fit diagram to width');
  private svg?: SVGSVGElement;
  private width = 0;
  private height = 0;
  private scale = 1;
  private fitted = true;
  private resize: ResizeObserver;
  private drag?: { id: number; x: number; y: number; left: number; top: number };

  constructor(private viewport: HTMLElement) {
    this.controls.className = 'diagram-zoom';
    this.controls.setAttribute('role', 'group'); this.controls.setAttribute('aria-label', 'Diagram zoom');
    this.controls.append(this.out, this.reset, this.increase, this.fit);
    viewport.tabIndex = 0;
    viewport.setAttribute('role', 'region');
    viewport.setAttribute('aria-label', 'Mermaid diagram. Use plus or minus to zoom, zero to reset, or drag to pan.');
    this.out.addEventListener('click', () => this.zoom(this.scale / 1.25));
    this.increase.addEventListener('click', () => this.zoom(this.scale * 1.25));
    this.reset.addEventListener('click', () => this.zoom(1));
    this.fit.addEventListener('click', () => { this.fitted = true; this.layout(); viewport.scrollTo(0, 0); });
    viewport.addEventListener('wheel', event => {
      if (!(event.ctrlKey || event.metaKey) || !this.svg) return;
      event.preventDefault();
      const rect = viewport.getBoundingClientRect();
      this.zoom(this.scale * (event.deltaY < 0 ? 1.15 : 1 / 1.15), event.clientX - rect.left, event.clientY - rect.top);
    }, { passive: false });
    viewport.addEventListener('keydown', event => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (['+', '=', '-', '0'].includes(event.key)) {
        event.preventDefault(); event.stopPropagation();
        this.zoom(event.key === '0' ? 1 : this.scale * (event.key === '-' ? 1 / 1.25 : 1.25));
      }
    });
    viewport.addEventListener('pointerdown', event => {
      if (event.button !== 0 || !this.svg || (viewport.scrollWidth <= viewport.clientWidth && viewport.scrollHeight <= viewport.clientHeight)) return;
      this.drag = { id: event.pointerId, x: event.clientX, y: event.clientY, left: viewport.scrollLeft, top: viewport.scrollTop };
      viewport.setPointerCapture(event.pointerId); viewport.classList.add('panning'); event.preventDefault();
    });
    viewport.addEventListener('pointermove', event => {
      if (!this.drag || this.drag.id !== event.pointerId) return;
      viewport.scrollLeft = this.drag.left + this.drag.x - event.clientX;
      viewport.scrollTop = this.drag.top + this.drag.y - event.clientY;
    });
    const end = () => { this.drag = undefined; viewport.classList.remove('panning'); };
    viewport.addEventListener('pointerup', end); viewport.addEventListener('pointercancel', end); viewport.addEventListener('lostpointercapture', end);
    this.resize = new ResizeObserver(() => { if (this.fitted) this.layout(); });
    this.resize.observe(viewport);
    this.layout();
  }
  private button(text: string, label: string) {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = text;
    button.title = label; button.setAttribute('aria-label', label); return button;
  }
  rendered() {
    this.svg = this.viewport.querySelector('svg') ?? undefined;
    const box = this.svg?.viewBox.baseVal;
    this.width = box?.width || 0; this.height = box?.height || 0;
    this.layout();
  }
  private layout() {
    const ready = !!this.svg && this.width > 0 && this.height > 0;
    if (ready) {
      const available = Math.max(1, this.viewport.clientWidth - 44);
      if (this.fitted) this.scale = Math.min(1, available / this.width);
      Object.assign(this.svg!.style, { maxWidth: 'none', width: `${this.width * this.scale}px`, height: `${this.height * this.scale}px` });
    }
    this.reset.textContent = `${Math.round(this.scale * 100)}%`;
    this.out.disabled = !ready || this.scale <= 0.1;
    this.increase.disabled = !ready || this.scale >= 4;
    this.reset.disabled = this.fit.disabled = !ready;
  }
  private zoom(scale: number, x = this.viewport.clientWidth / 2, y = this.viewport.clientHeight / 2) {
    if (!this.svg) return;
    const before = this.scale;
    this.scale = Math.max(0.1, Math.min(4, scale)); this.fitted = false;
    const left = (this.viewport.scrollLeft + x) * this.scale / before - x;
    const top = (this.viewport.scrollTop + y) * this.scale / before - y;
    this.layout(); this.viewport.scrollTo(left, top);
  }
  destroy() { this.resize.disconnect(); }
}
