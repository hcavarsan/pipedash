import { useEffect, useState } from 'react'

export function useScrolledPast(threshold: number): boolean {
  const [scrolled, setScrolled] = useState(false)

  useEffect(() => {
    const onScroll = (event: Event) => {
      const target = event.target
      const main = document.querySelector('.mantine-AppShell-main')

      if (target === document || target === main) {
        const top = target === document ? window.scrollY : (target as HTMLElement).scrollTop

        setScrolled(top > threshold)
      }
    }

    document.addEventListener('scroll', onScroll, { capture: true, passive: true })

    return () => document.removeEventListener('scroll', onScroll, { capture: true })
  }, [threshold])

  return scrolled
}
