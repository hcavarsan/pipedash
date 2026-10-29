import type { Win } from './logWindowTypes'

export function measureRenderedRows(scroller: HTMLElement | null, win: Win | null): void {
  if (!win) {
    return
  }

  for (const node of scroller?.querySelectorAll<HTMLElement>('[data-row]') ?? []) {
    const rel = Number(node.dataset.row) - win.base
    const height = node.getBoundingClientRect().height

    if (rel >= 0 && rel < win.size && height > 0) {
      win.heights.set(rel, height)
    }
  }
}
