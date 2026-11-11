import { type Dispatch, type RefObject, useCallback } from 'react'
import { service } from '../../../../services/service'
import { displayErrorNotification } from '../../../../utils/errorDisplay'

import type { FormAction } from './types'

interface SelectEveryMatchParams {
  dispatch: Dispatch<FormAction>
  providerType: string
  config: Record<string, string>
  organization: string
  search: string
  listChanges: RefObject<number>
}

export function useSelectEveryMatch({
  dispatch,
  providerType,
  config,
  organization,
  search,
  listChanges,
}: SelectEveryMatchParams): (run: number) => Promise<void> {
  return useCallback(
    async (run: number): Promise<void> => {
      const listVersion = listChanges.current
      let pipelineIds: string[] | null = null

      dispatch({ type: 'START_SELECT_ALL', run })
      try {
        const ids = await service.getMatchingPipelineIds(
          providerType,
          config,
          organization || undefined,
          search || undefined
        )

        pipelineIds = listChanges.current === listVersion ? ids : null
      } catch (err: unknown) {
        displayErrorNotification(err, "Couldn't select all")
      } finally {
        dispatch({ type: 'FINISH_SELECT_ALL', run, pipelineIds })
      }
    },
    [dispatch, providerType, config, organization, search, listChanges]
  )
}
