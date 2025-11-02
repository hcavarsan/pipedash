export const formatDuration = (seconds: number | null): string => {
  if (!seconds || seconds === 0) {
    return '-'
  }

  if (seconds < 0) {
    return '-'
  }

  if (seconds < 60) {
    return `${Math.round(seconds)}s`
  }

  if (seconds < 3600) {
    const mins = Math.floor(seconds / 60)
    const secs = Math.round(seconds % 60)

    return secs > 0 ? `${mins}m ${secs}s` : `${mins}m`
  }

  const hours = Math.floor(seconds / 3600)
  const mins = Math.round((seconds % 3600) / 60)

  return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`
}
