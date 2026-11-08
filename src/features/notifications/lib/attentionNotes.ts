import type { HealthSignal } from '../../../types/health'
import type { AttentionSchedule, ProviderSummary } from '../../../types/providers'
import { formatTime } from '../../../utils/formatDateTime'

const scheduleNote = (schedule: AttentionSchedule | null): string | null => {
  if (!schedule) {
    return null
  }

  return schedule.type === 'checking_now' ? 'Checking now' : `Next attempt after ${formatTime(schedule.at)}`
}

const staleNote = (signal: HealthSignal): string => {
  const source = signal.kind === 'status_page' ? 'Status page' : signal.reported_by

  return `${source} unreachable since ${formatTime(signal.observed_at)}. Last report: ${signal.title}.`
}

export const connectionNotes = ({ attention }: ProviderSummary): string[] =>
  [attention.fetch_hint, scheduleNote(attention.schedule)].filter((note): note is string => Boolean(note))

export const staleNotes = ({ attention }: ProviderSummary): string[] => attention.stale_signals.map(staleNote)
