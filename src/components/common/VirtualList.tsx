import { useVirtualizer } from '@tanstack/react-virtual'
import { type ReactNode, useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from 'react'

import { findScrollParent } from '../../utils/scrollParent'

interface VirtualListProps<T> {
  items: readonly T[]
  getKey: (item: T) => string
  estimateSize: number
  renderItem: (item: T, index: number) => ReactNode
  onEndReached?: (() => void) | undefined
  className?: string | undefined
}

interface ScrollTarget {
  element: HTMLElement
  margin: number
}

const END_THRESHOLD = 5

function offsetWithin(list: HTMLElement, scroller: HTMLElement): number {
  const documentScroller = scroller === document.scrollingElement || scroller === document.documentElement
  const base = documentScroller ? 0 : scroller.getBoundingClientRect().top

  return list.getBoundingClientRect().top - base + scroller.scrollTop
}

export function VirtualList<T>({
  items,
  getKey,
  estimateSize,
  renderItem,
  onEndReached,
  className,
}: VirtualListProps<T>): ReactNode {
  const listRef = useRef<HTMLDivElement>(null)
  const [scroll, setScroll] = useState<ScrollTarget | null>(null)

  useLayoutEffect(() => {
    const list = listRef.current
    if (!list) {
      return
    }
    const element = findScrollParent(list)
    const measure = () => {
      const margin = offsetWithin(list, element)
      setScroll((previous) =>
        previous?.element === element && previous.margin === margin ? previous : { element, margin }
      )
    }

    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    if (list.parentElement) {
      observer.observe(list.parentElement)
    }

    return () => observer.disconnect()
  }, [])

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroll?.element ?? null,
    estimateSize: () => estimateSize,
    overscan: 8,
    scrollMargin: scroll?.margin ?? 0,
    getItemKey: (index) => {
      const item = items[index]

      return item === undefined ? index : getKey(item)
    },
    initialRect: { width: window.innerWidth, height: window.innerHeight },
  })

  const rows = virtualizer.getVirtualItems()
  const lastIndex = rows.at(-1)?.index ?? -1
  const reachEnd = useEffectEvent(() => {
    onEndReached?.()
  })

  useEffect(() => {
    if (items.length > 0 && lastIndex >= items.length - END_THRESHOLD) {
      reachEnd()
    }
  }, [lastIndex, items.length])

  return (
    <div ref={listRef} className={className} style={{ position: 'relative', height: virtualizer.getTotalSize() }}>
      {rows.map((row) => {
        const item = items[row.index]

        if (item === undefined) {
          return null
        }

        return (
          <div
            key={row.key}
            data-index={row.index}
            data-virtual-item
            data-first={row.index === 0 || undefined}
            data-last={row.index === items.length - 1 || undefined}
            ref={virtualizer.measureElement}
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: '100%',
              transform: `translateY(${row.start - virtualizer.options.scrollMargin}px)`,
            }}
          >
            {renderItem(item, row.index)}
          </div>
        )
      })}
    </div>
  )
}
