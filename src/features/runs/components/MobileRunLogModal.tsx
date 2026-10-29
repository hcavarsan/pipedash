import type { ReactNode } from 'react'
import { StandardModal } from '../../../components/common/StandardModal'
import type { RunLogNode } from '../../../types/logs'
import { LogViewer } from '../../logs/components/LogViewer'

interface MobileRunLogModalProps {
  pipelineId: string
  pipelineName: string
  runNumber: number
  providerName: string
  nodes: readonly RunLogNode[]
  selectedId: string | null
  onClose: () => void
}

export function MobileRunLogModal({
  pipelineId,
  pipelineName,
  runNumber,
  providerName,
  nodes,
  selectedId,
  onClose,
}: MobileRunLogModalProps): ReactNode {
  const node = nodes.find((candidate) => candidate.id === selectedId && candidate.log !== null)

  if (!node) {
    return null
  }

  return (
    <StandardModal
      opened
      onClose={onClose}
      title={node.name}
      backAction={{ label: `#${runNumber}`, onClick: onClose }}
      disableScrollArea
      contentPadding={false}
      fullHeight
    >
      <LogViewer
        pipelineId={pipelineId}
        pipelineName={pipelineName}
        runNumber={runNumber}
        node={node}
        steps={nodes.filter((candidate) => candidate.parent_id === node.id && candidate.kind === 'step')}
        variant="phone"
        providerName={providerName}
      />
    </StandardModal>
  )
}
