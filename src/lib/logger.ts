
const isDev = import.meta.env.DEV

function formatMessage(component: string, message: string, data?: unknown): string[] {
  const parts = [`[${component}]`, message]

  if (data !== undefined) {
    parts.push(typeof data === 'object' ? JSON.stringify(data, null, 2) : String(data))
  }

  return parts
}

export const logger = {
  debug: (component: string, message: string, data?: unknown): void => {
    if (isDev) {
      console.debug(...formatMessage(component, message, data))
    }
  },

  info: (component: string, message: string, data?: unknown): void => {
    console.info(...formatMessage(component, message, data))
  },

  warn: (component: string, message: string, data?: unknown): void => {
    console.warn(...formatMessage(component, message, data))
  },

  error: (component: string, message: string, data?: unknown): void => {
    console.error(...formatMessage(component, message, data))
  },

  time: (label: string): (() => void) => {
    if (!isDev) {
      return () => undefined
    }

    const start = performance.now()

    return () => {
      const duration = performance.now() - start

      console.debug(`[Timer:${label}]`, `${duration.toFixed(2)}ms`)
    }
  },

  group: (label: string, fn: () => void): void => {
    if (!isDev) {
      fn()

      return
    }

    console.group(label)
    fn()
    console.groupEnd()
  },
}

