import { Button, Group, Tooltip } from '@mantine/core'
import { IconAdjustments } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { PageHeader } from '../../../components/common/PageHeader'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import type { Pipeline } from '../../../types/pipelines'
import { triggerIcon } from '../../../utils/pipelineRowActions'
import { PipelineNotifyMenu } from '../../notifications/components/PipelineNotifyMenu'
import { useRunActions } from './RunActionsContext'

interface RunHistoryHeaderProps {
  pipeline: Pipeline
  showCustomize: boolean
  customizationDisabled: boolean
  onBack: () => void
  onCustomize: () => void
}

function HeaderActions({
  pipeline,
  showCustomize,
  customizationDisabled,
  onCustomize,
}: Omit<RunHistoryHeaderProps, 'onBack'>): ReactNode {
  const { labels, openTrigger, triggerBlockedReason } = useRunActions()

  return (
    <Group gap="sm" wrap="nowrap" style={{ minWidth: 'fit-content' }}>
      {showCustomize && (
        <Button
          type="button"
          variant="default"
          size="sm"
          leftSection={<IconAdjustments size={16} aria-hidden />}
          onClick={onCustomize}
          disabled={customizationDisabled}
        >
          Customize columns
        </Button>
      )}
      <PipelineNotifyMenu pipeline={pipeline} />
      <Tooltip label={triggerBlockedReason} disabled={!triggerBlockedReason} multiline w={280}>
        <Button
          type="button"
          size="sm"
          leftSection={triggerIcon(pipeline)}
          onClick={openTrigger}
          {...(triggerBlockedReason ? { 'aria-disabled': true, 'data-disabled': true } : {})}
        >
          {labels.trigger}
        </Button>
      </Tooltip>
    </Group>
  )
}

export function RunHistoryHeader(props: RunHistoryHeaderProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()

  return (
    <PageHeader
      title={props.pipeline.name}
      onBack={props.onBack}
      actions={mobileShell ? undefined : <HeaderActions {...props} showCustomize={props.showCustomize && !isMobile} />}
    />
  )
}
