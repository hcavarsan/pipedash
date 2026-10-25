export interface LabelExtent {
  scrollWidth: number
  clientWidth: number
}

const LABEL_SELECTOR = '.mantine-Button-label'

export const labelsFit = (labels: readonly LabelExtent[]): boolean =>
  labels.every(({ scrollWidth, clientWidth }) => scrollWidth <= clientWidth + 1)

function fitActions(group: HTMLElement) {
  group.dataset.layout = 'row'

  if (!labelsFit(Array.from(group.querySelectorAll<HTMLElement>(LABEL_SELECTOR)))) {
    group.dataset.layout = 'column'
  }
}

export function watchActions(group: HTMLElement): () => void {
  fitActions(group)

  const observer = new ResizeObserver(() => fitActions(group))

  observer.observe(group)

  return () => observer.disconnect()
}
