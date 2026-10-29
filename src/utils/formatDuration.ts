export const formatDuration = (seconds: number | null | undefined): string => {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) {
    return '—'
  }

  const total = Math.round(seconds)

  if (total < 60) {
    return `${total}s`
  }

  if (total < 3600) {
    const mins = Math.floor(total / 60)
    const secs = total % 60

    return secs > 0 ? `${mins}m ${secs}s` : `${mins}m`
  }

  const hours = Math.floor(total / 3600)
  const mins = Math.round((total % 3600) / 60)

  return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`
}
