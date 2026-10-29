const SCROLLABLE: readonly string[] = ['auto', 'scroll', 'overlay']

export function findScrollParent(node: HTMLElement): HTMLElement {
  let current = node.parentElement

  while (current) {
    if (SCROLLABLE.includes(getComputedStyle(current).overflowY)) {
      return current
    }
    current = current.parentElement
  }

  const root = document.scrollingElement

  return root instanceof HTMLElement ? root : document.documentElement
}
