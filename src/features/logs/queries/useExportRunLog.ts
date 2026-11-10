import { type UseMutationResult, useMutation } from '@tanstack/react-query'

import { isTauri } from '../../../services/isTauri'
import { service } from '../../../services/service'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'

interface ExportRunLogInput {
  pipelineId: string
  runNumber: number
  nodeId: string
  revision: number
  fileName: string
}

export function useExportRunLog(): UseMutationResult<boolean, Error, ExportRunLogInput, unknown> {
  return useMutation({
    mutationFn: ({ pipelineId, runNumber, nodeId, revision, fileName }: ExportRunLogInput) =>
      service.exportRunLog(pipelineId, runNumber, nodeId, revision, fileName),
    onSuccess: (saved) => {
      if (saved) {
        displaySuccessNotification(isTauri() ? 'The log file was saved.' : 'The log download started.', 'Log exported')
      }
    },
    onError: (error) => displayErrorNotification(error, "Couldn't export the log"),
  })
}
