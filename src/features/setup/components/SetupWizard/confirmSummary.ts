export const summarizeConnection = (url: string): string | null => {
  try {
    const { host, pathname } = new URL(url)

    return host ? `${host}${pathname}` : null
  } catch {
    return null
  }
}

export const truncatePath = (path: string): string => {
  const segments = path.split('/')

  if (segments.length <= 2) {
    return path
  }

  return `.../${segments.slice(-2).join('/')}`
}
