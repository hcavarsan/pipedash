import { Button, Group, Text, Tooltip } from '@mantine/core'
import { IconAdjustments } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { PageHeader } from '../../../components/common/PageHeader'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { STATUS_META } from '../../../theme/status'
import type { Pipeline } from '../../../types/pipelines'
import { getPipelineStatusLabel } from '../../../utils/pipelinePresentation'
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

function StateTitle({ pipeline }: { pipeline: Pipeline }): ReactNode {
  return (
    <Text component="span" fw={600} c={`var(--status-${STATUS_META[pipeline.status].tone})`}>
      {getPipelineStatusLabel(pipeline) ?? STATUS_META[pipeline.status].label}
    </Text>
  )
}

export function RunHistoryHeader(props: RunHistoryHeaderProps): ReactNode {
  const { pipeline, onBack } = props
  const { presentation } = useRunActions()
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const state = pipeline.resource_state?.message ? pipeline : null

  return (
    <>
      <PageHeader
        title={pipeline.name}
        onBack={onBack}
        actions={
          mobileShell ? undefined : <HeaderActions {...props} showCustomize={props.showCustomize && !isMobile} />
        }
      />
      {state && mobileShell && (
        <IosGroup>
          <IosRow title={<StateTitle pipeline={state} />} subtitle={state.resource_state?.message} />
        </IosGroup>
      )}
      {state && !mobileShell && (
        <Group gap="sm" align="flex-start" mb="sm">
          <StatusBadge status={state.status} label={getPipelineStatusLabel(state)} size="md" />
          <Text size="sm" style={{ overflowWrap: 'anywhere', flex: 1 }}>
            {state.resource_state?.message}
          </Text>
        </Group>
      )}
      {presentation.history_note && (
        <Text size={mobileShell ? 'xs' : 'sm'} c="dimmed" {...(mobileShell ? { px: 'md' } : { mb: 'sm' })}>
          {presentation.history_note}
        </Text>
      )}
    </>
  )
}
