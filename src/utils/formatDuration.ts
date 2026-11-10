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

  const totalMins = Math.round(total / 60)
  const hours = Math.floor(totalMins / 60)
  const mins = totalMins % 60

  return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`
}
