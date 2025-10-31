import { createContext, ReactNode, useContext, useEffect, useMemo, useState } from 'react'

interface MediaQueryContextValue {
  isMobile: boolean;
}

const MediaQueryContext = createContext<MediaQueryContextValue | undefined>(undefined)

export const MediaQueryProvider = ({ children }: { children: ReactNode }) => {
  const getIsMobile = () => {
    if (typeof window === 'undefined') {
return false
}

return window.innerWidth <= 768
  }

  const [isMobile, setIsMobile] = useState(getIsMobile)

  useEffect(() => {
    let rafId: number | null = null
    let lastCheck = 0
    const THROTTLE_MS = 300
    const handleResize = () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId)
      }

      rafId = requestAnimationFrame(() => {
        const now = Date.now()

        if (now - lastCheck >= THROTTLE_MS) {
          lastCheck = now
          const newIsMobile = getIsMobile()

          setIsMobile(prevIsMobile => {
            if (prevIsMobile !== newIsMobile) {
              return newIsMobile
            }

return prevIsMobile
          })
        }

        rafId = null
      })
    }

    window.addEventListener('resize', handleResize, { passive: true })

    return () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId)
      }
      window.removeEventListener('resize', handleResize)
    }
  }, [])

  const value = useMemo(() => ({ isMobile }), [isMobile])

  return (
    <MediaQueryContext.Provider value={value}>
      {children}
    </MediaQueryContext.Provider>
  )
}

export const useIsMobile = () => {
  const context = useContext(MediaQueryContext)


  if (context === undefined) {
    throw new Error('useIsMobile must be used within MediaQueryProvider')
  }

return context.isMobile
}
