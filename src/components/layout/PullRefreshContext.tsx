import { createContext, type ReactNode, use, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'

export type RefreshHandler = () => Promise<unknown>

interface Registration {
  run: RefreshHandler
}

interface PullRefreshRegistry {
  register: (run: RefreshHandler) => () => void
}

interface PullRefreshState {
  registered: RefreshHandler | null
  registry: PullRefreshRegistry
}

export const PullRefreshContext = createContext<PullRefreshRegistry | null>(null)

export function usePullRefreshState(): PullRefreshState {
  const [registration, setRegistration] = useState<Registration | null>(null)
  const registry = useMemo<PullRefreshRegistry>(
    () => ({
      register: (run) => {
        const entry: Registration = { run }

        setRegistration(entry)

        return () => setRegistration((current) => (current === entry ? null : current))
      },
    }),
    []
  )

  return { registered: registration?.run ?? null, registry }
}

function usePullRefreshRegistry(): PullRefreshRegistry {
  const registry = use(PullRefreshContext)

  if (registry === null) {
    throw new Error('usePullToRefresh must be used inside MobilePage')
  }

  return registry
}

export function usePullToRefresh(refresh: RefreshHandler | null): void {
  const registry = usePullRefreshRegistry()
  const latest = useRef(refresh)
  const enabled = refresh !== null

  useLayoutEffect(() => {
    latest.current = refresh
  }, [refresh])

  useEffect(() => {
    if (!enabled) {
      return
    }

    return registry.register(() => latest.current?.() ?? Promise.resolve())
  }, [enabled, registry])
}

export function PullToRefresh({ refresh }: { refresh: RefreshHandler }): ReactNode {
  usePullToRefresh(refresh)

  return null
}
