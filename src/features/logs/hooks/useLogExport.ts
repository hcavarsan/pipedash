import { useClipboard } from '@mantine/hooks'
import { service } from '../../../services/service'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import type { DocRef } from '../lib/lineCacheTypes'
import { collectLines, exportFileName, FULL_LINE_UNITS } from '../lib/logActions'
import { useExportRunLog } from '../queries/useExportRunLog'
import type { LogSelection } from './useLogNavigation'

interface LogExportOptions {
  doc: DocRef
  pipelineName: string
  nodeName: string
  selection: LogSelection | null
}

export interface LogExportState {
  copyRange: (from: number, to: number, title: string) => Promise<void>
  copySelection: () => void
  exportLog: () => Promise<void>
  exporting: boolean
  copied: boolean
}

export function useLogExport({ doc, pipelineName, nodeName, selection }: LogExportOptions): LogExportState {
  const { pipelineId, runNumber, nodeId, revision } = doc
  const clipboard = useClipboard({ timeout: 2000 })
  const { mutateAsync, isPending } = useExportRunLog()

  const copyRange = async (from: number, to: number, title: string) => {
    try {
      const text = await collectLines(from, to, (start, count) =>
        service.readRunLogLines(pipelineId, runNumber, nodeId, revision, start, count, FULL_LINE_UNITS)
      )

      clipboard.copy(text)
    } catch (error) {
      displayErrorNotification(error, title)
    }
  }

  const copySelection = () => {
    if (selection) {
      void copyRange(selection.from, selection.to + 1, "Couldn't copy the lines")
    }
  }

  const exportLog = async () => {
    try {
      await mutateAsync({
        pipelineId,
        runNumber,
        nodeId,
        revision,
        fileName: exportFileName(pipelineName, runNumber, nodeName),
      })
    } catch {}
  }

  return { copyRange, copySelection, exportLog, exporting: isPending, copied: clipboard.copied }
}
