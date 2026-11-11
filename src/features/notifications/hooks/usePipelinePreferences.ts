import type { UseQueryResult } from '@tanstack/react-query'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useNotificationStore } from '../../../stores/notificationStore'
import type { PipelineNotifyState } from '../../../types/notification_preferences'
import { changePreference } from '../queries/changePreference'

export interface PipelinePreference {
  key: string
  name: string
  detail: string
  action: string
  label: string
  run: () => void
}

interface PipelinePreferences {
  muted: PipelinePreference[]
  watching: PipelinePreference[]
  lookup: UseQueryResult<unknown, Error>
}

const idsIn = (states: Record<string, PipelineNotifyState>, wanted: PipelineNotifyState): string[] =>
  Object.entries(states).flatMap(([id, state]) => (state === wanted ? [id] : []))

export function usePipelinePreferences(): PipelinePreferences {
  const pipelineStates = useNotificationStore((state) => state.pipelineStates)
  const { data: providers = [] } = useProviders()
  const pipelinesQuery = usePipelines()
  const { data: pipelines = [] } = pipelinesQuery

  const describe = (id: string): { name: string; detail: string } => {
    const pipeline = pipelines.find((item) => item.id === id)
    const provider = providers.find((item) => item.id === pipeline?.provider_id)
    let missing = 'Details unavailable'

    if (pipelinesQuery.isSuccess) {
      missing = 'No longer in Pipedash'
    }

    return {
      name: pipeline?.name ?? id,
      detail: pipeline
        ? [pipeline.repository, provider?.name ?? pipeline.provider_type].filter(Boolean).join(' · ')
        : missing,
    }
  }

  const muted = idsIn(pipelineStates, 'muted').map((id): PipelinePreference => {
    const { name, detail } = describe(id)

    return {
      key: id,
      name,
      detail,
      action: 'Unmute',
      label: `Unmute ${name}`,
      run: () => void changePreference({ type: 'pipeline_muted', pipeline_id: id, muted: false }),
    }
  })

  const watched = idsIn(pipelineStates, 'watching').map((id): PipelinePreference => {
    const { name, detail } = describe(id)

    return {
      key: `watch:${id}`,
      name,
      detail: `${detail} · Every finished run`,
      action: 'Stop watching',
      label: `Stop watching ${name}`,
      run: () => void changePreference({ type: 'pipeline_watched', pipeline_id: id, watched: false }),
    }
  })

  const pending = idsIn(pipelineStates, 'pending').map((id): PipelinePreference => {
    const { name, detail } = describe(id)

    return {
      key: `once:${id}`,
      name,
      detail: `${detail} · Next finish only`,
      action: 'Cancel',
      label: `Cancel finish notification for ${name}`,
      run: () => void changePreference({ type: 'cancel_next_finish', pipeline_id: id }),
    }
  })

  return { muted, watching: [...watched, ...pending], lookup: pipelinesQuery }
}
