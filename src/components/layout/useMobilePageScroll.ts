import { type RefObject, useCallback, useEffect, useRef } from 'react'

import { scrollEdge, titleCollapse } from './navigation'
import type { MobilePageLayers } from './useMobilePageMotion'

interface MobilePageScroll {
  scrollTop: RefObject<number>
  collapse: RefObject<number>
  measure: () => void
}

export const scrollerOf = (host: HTMLElement | null): HTMLElement | null =>
  host?.closest<HTMLElement>('.mantine-AppShell-main') ?? null

export function useMobilePageScroll({ host, page }: Pick<MobilePageLayers, 'host' | 'page'>): MobilePageScroll {
  const scrollTop = useRef(0)
  const collapse = useRef(0)

  const measure = useCallback(() => {
    const root = document.documentElement
    const title = page.current?.querySelector('[data-large-title]')
    const barBottom = document.querySelector('.mantine-AppShell-header')?.getBoundingClientRect().bottom ?? 0
    let next = 1

    if (title) {
      const rect = title.getBoundingClientRect()

      next = titleCollapse(rect.bottom, barBottom, rect.height)
    }

    collapse.current = next
    root.style.setProperty('--title-collapse', String(next))
    root.style.setProperty('--scroll-edge', String(scrollEdge(scrollTop.current)))
  }, [page])

  useEffect(() => {
    const main = scrollerOf(host.current)
    let frame = 0

    const onScroll = () => {
      scrollTop.current = main?.scrollTop ?? 0
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0
          measure()
        })
      }
    }

    measure()
    main?.addEventListener('scroll', onScroll, { passive: true })
    window.addEventListener('resize', measure)

    return () => {
      cancelAnimationFrame(frame)
      main?.removeEventListener('scroll', onScroll)
      window.removeEventListener('resize', measure)
    }
  }, [host, measure])

  return { scrollTop, collapse, measure }
}
