import { Box, Button, Group, Skeleton, Text, Tooltip } from '@mantine/core'
import { IconAdjustments } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { PageHeader } from '../../../components/common/PageHeader'
import { triggerIcon } from '../../../components/common/PipelineRowParts'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { STATUS_META } from '../../../theme/status'
import type { Pipeline } from '../../../types/domain'
import { getPipelineStatusLabel } from '../../../utils/pipelinePresentation'
import { useRunActions } from './RunActionsContext'

interface RunHistoryHeaderProps {
  pipeline: Pipeline | null
  isLoadingPipeline: boolean
  showCustomize: boolean
  customizationDisabled: boolean
  onBack: () => void
  onCustomize: () => void
}

function HeaderActions({
  pipeline,
  isLoadingPipeline,
  showCustomize,
  customizationDisabled,
  onCustomize,
}: Omit<RunHistoryHeaderProps, 'onBack'>): ReactNode {
  const { labels, openTrigger, triggerBlockedReason } = useRunActions()

  if (isLoadingPipeline || !pipeline) {
    return (
      <Box style={{ minWidth: 'fit-content' }}>
        <Skeleton height="2.25rem" width="10rem" />
      </Box>
    )
  }

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
  const { pipeline, isLoadingPipeline, onBack } = props
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const state = pipeline?.resource_state && pipeline

  return (
    <>
      <PageHeader
        title={
          !isLoadingPipeline && pipeline?.name ? pipeline.name : <Skeleton height="1.5rem" width="min(250px, 60vw)" />
        }
        onBack={onBack}
        actions={
          mobileShell ? undefined : <HeaderActions {...props} showCustomize={props.showCustomize && !isMobile} />
        }
      />
      {state && mobileShell && (
        <IosGroup>
          <IosRow
            title={
              <Text component="span" fw={600} c={`var(--status-${STATUS_META[state.status].tone})`}>
                {getPipelineStatusLabel(state) ?? STATUS_META[state.status].label}
              </Text>
            }
            subtitle={state.resource_state?.message}
          />
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
      {pipeline?.metadata?.history_scope === 'bounded_reconciliation' && (
        <Text size={mobileShell ? 'xs' : 'sm'} c="dimmed" {...(mobileShell ? { px: 'md' } : { mb: 'sm' })}>
          FluxCD retains a limited reconciliation history. Repeated reconciliations may be grouped, and older entries
          may be unavailable.
        </Text>
      )}
    </>
  )
}
