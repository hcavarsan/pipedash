interface Snapshot {
  node: HTMLElement
  scroll: number
  collapse: number
}

const LIMIT = 6
const cache = new Map<string, Snapshot>()

export function captureSnapshot(key: string, source: HTMLElement, scroll: number, collapse: number): void {
  const node = source.cloneNode(true) as HTMLElement

  node.style.transform = ''
  cache.delete(key)
  cache.set(key, { node, scroll, collapse })

  for (const oldest of cache.keys()) {
    if (cache.size <= LIMIT) {
      break
    }
    cache.delete(oldest)
  }
}

export function updateSnapshot(key: string, scroll: number, collapse: number): void {
  const entry = cache.get(key)

  if (entry) {
    entry.scroll = scroll
    entry.collapse = collapse
  }
}

export function snapshotCollapse(key: string): number {
  return cache.get(key)?.collapse ?? 0
}

export function mountSnapshot(container: HTMLElement, key: string, title: string, placeholderClass: string): void {
  const entry = cache.get(key)
  const layer = document.createElement('div')

  if (entry) {
    layer.style.transform = `translateY(${-entry.scroll}px)`
    layer.style.setProperty('--title-collapse', String(entry.collapse))
    layer.append(entry.node)
  } else {
    const heading = document.createElement('div')

    heading.className = placeholderClass
    heading.textContent = title
    layer.append(heading)
  }

  container.replaceChildren(layer)
}
