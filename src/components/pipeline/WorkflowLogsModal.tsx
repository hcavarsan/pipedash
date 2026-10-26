import {
  Alert,
  Box,
  Button,
  Code,
  Divider,
  Group,
  Loader,
  Paper,
  ScrollArea,
  Stack,
  Tabs,
  Text,
  VisuallyHidden,
} from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCopy, IconExternalLink, IconPlayerPlay, IconRefresh, IconShare2, IconSquare } from '@tabler/icons-react'
import { Fragment, useEffect, useMemo, useRef, useState } from 'react'

import { usePlugins } from '../../contexts/PluginContext'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { useRerunWorkflow, useRunDetails } from '../../queries/useRunDetailsQuery'
import { useCancelRun, useRerunRun } from '../../queries/useRunHistoryQueries'
import { useRunLogTree } from '../../queries/useRunLogQueries'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import { service } from '../../services'
import { mobileService } from '../../services/mobile'
import { STATUS_META } from '../../theme/status'
import { TYPOGRAPHY } from '../../theme/typography'
import type { CellRenderer, PipelineStatus } from '../../types'
import { filterVisibleColumns } from '../../utils/columnBuilder'
import { type CellValue, renderCellValue } from '../../utils/dynamicRenderers'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatDuration } from '../../utils/formatDuration'
import { getValueByPath } from '../../utils/objectPath'
import { isHttpUrl, openExternal } from '../../utils/openExternal'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { isMobilePlatform } from '../../utils/platform'
import { CopyButton } from '../atoms/CopyButton'
import { StatusBadge, StatusIcon } from '../atoms/StatusBadge'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'
import { LogViewer, type LogViewerHandle } from '../logs/LogViewer'

import { BuildWatchControls } from './BuildWatchControls'
import { createLogShortcutHandler } from './logShortcuts'
import { groupLogNodes } from './logTree'
import { RunLogsPanel } from './RunLogsPanel'
import classes from './WorkflowLogsModal.module.css'

interface WorkflowLogsModalProps {
  opened: boolean
  onClose: () => void
  pipelineId: string
  runNumber: number
  providerId?: number
  providerType?: string
  pipelineName: string
  canViewLogs: boolean
  canTrigger: boolean
  triggerBlockedReason?: string
  canCancel: boolean
  rerunInPlace?: boolean
  onRerunSuccess?: (pipelineId: string, newRunNumber: number) => void
  onCancelSuccess?: () => void
}

interface DetailField {
  id: string
  label: string
  field_path: string
  renderer: CellRenderer
}

const FALLBACK_FIELDS: DetailField[] = [
  { id: 'run_number', label: 'Run number', field_path: 'run_number', renderer: 'Text' },
  { id: 'status', label: 'Status', field_path: 'status', renderer: 'StatusBadge' },
  { id: 'branch', label: 'Branch', field_path: 'branch', renderer: 'Text' },
  { id: 'duration_seconds', label: 'Duration', field_path: 'duration_seconds', renderer: 'Duration' },
  { id: 'started_at', label: 'Started at', field_path: 'started_at', renderer: 'DateTime' },
  { id: 'concluded_at', label: 'Concluded at', field_path: 'concluded_at', renderer: 'DateTime' },
  { id: 'actor', label: 'Actor', field_path: 'actor', renderer: 'Avatar' },
]

const isPipelineStatus = (value: unknown): value is PipelineStatus => typeof value === 'string' && value in STATUS_META

export const WorkflowLogsModal = ({
  opened,
  onClose,
  pipelineId,
  runNumber,
  providerId,
  providerType,
  canTrigger,
  triggerBlockedReason,
  canCancel,
  rerunInPlace,
  onRerunSuccess,
  onCancelSuccess,
  pipelineName,
  canViewLogs,
}: WorkflowLogsModalProps) => {
  const [cancelling, setCancelling] = useState(false)
  const [cancellationRequestedFor, setCancellationRequestedFor] = useState<string | null>(null)
  const cancelRun = useCancelRun()
  const clipboard = useClipboard({ timeout: 2000 })
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const abortRef = useRef(false)
  const labels = getPipelineLabels(providerType)
  const isFlux = providerType === 'fluxcd'
  const { getPluginDisplayName } = usePlugins()
  const providerName = providerType ? getPluginDisplayName(providerType) : 'provider'
  const [tab, setTab] = useState<'logs' | 'details'>('logs')
  const [logNodeId, setLogNodeId] = useState<string | null>(null)
  const viewerRef = useRef<LogViewerHandle>(null)
  const shortcuts = useMemo(() => createLogShortcutHandler(() => viewerRef.current), [])
  const { data: logTree } = useRunLogTree(pipelineId, runNumber, canViewLogs && mobileShell && opened)
  const logNode = logTree?.nodes.find((node) => node.id === logNodeId && node.log !== null)
  const jobGroups = canViewLogs && mobileShell ? groupLogNodes(logTree?.nodes ?? [], true) : []

  useEffect(() => {
    if (tab !== 'logs') {
      return
    }

    const frame = requestAnimationFrame(() => viewerRef.current?.focusBody())

    return () => cancelAnimationFrame(frame)
  }, [tab])

  useEffect(() => {
    abortRef.current = false

    return () => {
      abortRef.current = true
    }
  }, [])

  const {
    data: runDetails,
    isLoading: loading,
    isFetching,
    error: queryError,
    refetch,
  } = useRunDetails(pipelineId, runNumber, opened)

  const rerunMutation = useRerunWorkflow()
  const rerunRunMutation = useRerunRun()
  const rerunPending = rerunMutation.isPending || rerunRunMutation.isPending

  const { data: tableSchema } = useTableDefinition(providerId ?? 0, 'pipeline_runs')

  const fields = useMemo<DetailField[]>(() => {
    const columns = tableSchema ? filterVisibleColumns(tableSchema.columns).filter((col) => col.id !== 'actions') : []

    return columns.length > 0 ? columns : FALLBACK_FIELDS
  }, [tableSchema])

  const isRunning = runDetails?.status === 'running' || runDetails?.status === 'pending'
  const isPending = runDetails?.status === 'pending'

  const handleRerun = async () => {
    if (!runDetails) {
      return
    }

    try {
      if (rerunInPlace) {
        await rerunRunMutation.mutateAsync({ pipelineId, runNumber })
        onClose()
        onRerunSuccess?.(pipelineId, runNumber)

        return
      }

      const result = await rerunMutation.mutateAsync({
        pipelineId,
        inputs: runDetails.inputs,
      })

      onClose()

      if (onRerunSuccess && result.newRunNumber > 0) {
        onRerunSuccess(pipelineId, result.newRunNumber)
      }
    } catch (error: unknown) {
      logger.error('WorkflowLogsModal', 'Failed to re-run workflow', error)
    }
  }

  const handleCancel = async () => {
    if (!runDetails) {
      return
    }

    if (!mobileShell) {
      setCancelling(true)
      try {
        const requested = await cancelRun.mutateAsync({ pipelineId, runNumber })
        if (requested) {
          setCancellationRequestedFor(`${pipelineId}:${runNumber}`)
          await refetch()
          onCancelSuccess?.()
        }
      } catch {
        return
      } finally {
        setCancelling(false)
      }
      return
    }

    setCancelling(true)

    try {
      await service.cancelPipelineRun(pipelineId, runNumber)

      const maxAttempts = 10
      let attempt = 0
      let statusChanged = false

      while (attempt < maxAttempts && !statusChanged && !abortRef.current) {
        const { promise: delay, resolve: elapsed } = Promise.withResolvers<void>()

        setTimeout(elapsed, 1000)
        await delay

        if (abortRef.current) {
          break
        }

        try {
          const { data: freshDetails } = await refetch()

          if (freshDetails && freshDetails.status === 'cancelled') {
            statusChanged = true
          }
        } catch {
          /* refetch may fail during polling */
        }

        attempt++
      }

      if (abortRef.current) {
        return
      }

      if (!statusChanged) {
        await refetch()
      }

      if (onCancelSuccess) {
        onCancelSuccess()
      }
    } catch (error: unknown) {
      if (abortRef.current) {
        return
      }

      displayErrorNotification(error, `Couldn't cancel run #${runNumber}`)
    } finally {
      if (!abortRef.current) {
        setCancelling(false)
      }
    }
  }

  const handleShare = async (url: string, source: HTMLElement) => {
    if (isMobilePlatform()) {
      try {
        await mobileService.shareLink(url, source.getBoundingClientRect())
      } catch (error) {
        displayErrorNotification(error, "Couldn't share link")
      }

      return
    }

    if (typeof navigator.share !== 'function') {
      clipboard.copy(url)

      return
    }

    try {
      await navigator.share({ title: `Run #${runNumber}`, url })
    } catch (error: unknown) {
      if (!(error instanceof DOMException && error.name === 'AbortError')) {
        clipboard.copy(url)
      }
    }
  }

  const renderValue = (field: DetailField, value: unknown) => {
    if (field.id === 'status' && isPipelineStatus(value)) {
      return <StatusBadge status={value} label={runDetails ? getStatusLabel(runDetails) : undefined} size="md" />
    }

    if (field.id === 'message') {
      return (
        <Text size="sm" style={{ overflowWrap: 'anywhere' }}>
          {String(value)}
        </Text>
      )
    }

    if (field.id === 'run_number') {
      return (
        <Group gap="xs" wrap="nowrap">
          <Text size={TYPOGRAPHY.FIELD_VALUE.size}>#{String(value)}</Text>
          <CopyButton value={String(value)} label="Copy run number" />
        </Group>
      )
    }

    if ((field.id === 'id' || field.id === 'commit_sha') && typeof value === 'string') {
      const isCommit = field.id === 'commit_sha'

      return (
        <Group gap="xs" wrap="nowrap">
          <Code fz={TYPOGRAPHY.FIELD_VALUE_SMALL.size} style={{ wordBreak: 'break-all' }}>
            {isCommit ? value.substring(0, 7) : value}
          </Code>
          <CopyButton value={value} label={isCommit ? 'Copy commit SHA' : 'Copy run ID'} />
        </Group>
      )
    }

    return renderCellValue(field.renderer, value as CellValue)
  }

  const footer = runDetails ? (
    <Stack gap="xs">
      {isFlux && (
        <Text size="sm" c="dimmed">
          {triggerBlockedReason ??
            'Reconcile uses the current resource configuration. It does not restore this historical revision.'}
        </Text>
      )}
      {isPending && canCancel && (
        <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} c="dimmed">
          This run is pending. It can be stopped or rerun once it starts.
        </Text>
      )}
      <Group justify="flex-end" gap="sm" grow={isMobile}>
        {runDetails.logs_url && (
          <Button
            component="a"
            href={runDetails.logs_url}
            target="_blank"
            rel="noopener noreferrer"
            variant="default"
            size="sm"
            rightSection={<IconExternalLink size={16} />}
          >
            {canViewLogs ? `Open in ${providerName}` : isMobile ? 'Logs' : 'View full logs'}
          </Button>
        )}
        {isRunning && canCancel && (
          <Button
            color="red"
            size="sm"
            leftSection={<IconSquare size={16} />}
            onClick={handleCancel}
            loading={cancelling}
            disabled={isPending || (!mobileShell && cancellationRequestedFor === `${pipelineId}:${runNumber}`)}
          >
            {isMobile ? 'Stop' : 'Stop workflow'}
          </Button>
        )}
        {!isRunning && canTrigger && (
          <Button size="sm" leftSection={<IconPlayerPlay size={16} />} onClick={handleRerun} loading={rerunPending}>
            {isFlux ? labels.trigger : isMobile ? 'Rerun' : 'Rerun workflow'}
          </Button>
        )}
      </Group>
    </Stack>
  ) : null

  const renderContent = () => {
    if (queryError) {
      return (
        <ErrorState
          title={`Couldn't load run #${runNumber}`}
          error={queryError}
          onRetry={() => refetch()}
          retrying={isFetching}
        />
      )
    }

    if (loading || !runDetails) {
      return <LoadingState message="Loading run details..." />
    }

    const rows = fields.flatMap((field) => {
      const value = getValueByPath(runDetails, field.field_path)

      if ((value === null || value === undefined) && field.id !== 'status' && field.id !== 'run_number') {
        return []
      }

      return [{ field, value }]
    })

    const notices = (
      <>
        <BuildWatchControls pipelineId={pipelineId} runNumber={runNumber} status={runDetails.status} />
        {runDetails.metadata?.archived === true && (
          <Alert color="gray" title="Cached reconciliation">
            Flux no longer retains this entry. These details come from Pipedash's cache and will disappear if the cache
            is cleared.
          </Alert>
        )}
        {isRunning &&
          (mobileShell ? (
            <Alert
              color="blue"
              icon={<IconRefresh size={16} />}
              title={isFlux ? 'Reconciliation in progress' : 'Run in progress'}
            >
              <Text size={TYPOGRAPHY.HELPER_TEXT.size}>Details refresh automatically every 10 seconds.</Text>
            </Alert>
          ) : (
            <Alert color="gray" title={<StatusBadge status={runDetails.status} />}>
              <Text size={TYPOGRAPHY.HELPER_TEXT.size} role="status">
                {cancellationRequestedFor === `${pipelineId}:${runNumber}`
                  ? 'Cancellation requested. Waiting for the provider to confirm the final status.'
                  : 'Details refresh automatically every 10 seconds.'}
              </Text>
            </Alert>
          ))}
      </>
    )

    if (mobileShell) {
      const hasLogs = isHttpUrl(runDetails.logs_url)
      const stoppable = isRunning && canCancel
      const showRerun = !isRunning && canTrigger
      const note = isFlux
        ? (triggerBlockedReason ??
          'Reconcile uses the current resource configuration. It does not restore this historical revision.')
        : isPending && canCancel
          ? 'This run is pending. It can be stopped or rerun once it starts.'
          : undefined

      return (
        <Stack gap="lg">
          {notices}
          <div data-selectable>
            <IosGroup>
              {rows.map(({ field, value }) =>
                field.id === 'message' ||
                (field.renderer === 'Text' && typeof value === 'string' && value.length > 32) ? (
                  <IosRow key={field.id} title={renderValue(field, value)} subtitle={field.label} />
                ) : (
                  <IosRow
                    key={field.id}
                    title={field.label}
                    value={<span className={classes.value}>{renderValue(field, value)}</span>}
                  />
                )
              )}
            </IosGroup>
          </div>
          {jobGroups.map((group) => (
            <IosGroup key={group.stage ?? 'jobs'} header={group.stage ?? 'Jobs'}>
              {group.nodes.map((job) => (
                <IosRow
                  key={job.id}
                  icon={<StatusIcon status={job.status} />}
                  title={job.name}
                  value={formatDuration(job.duration_seconds)}
                  chevron
                  onClick={() => setLogNodeId(job.id)}
                />
              ))}
            </IosGroup>
          ))}
          {hasLogs && (
            <IosGroup footer={showRerun || stoppable ? undefined : note}>
              <IosRow
                title="View logs"
                accessory={<IconExternalLink size={18} />}
                onClick={() => openExternal(runDetails.logs_url)}
              />
              <IosRow
                title="Copy link"
                value={clipboard.copied ? 'Copied' : undefined}
                accessory={<IconCopy size={18} />}
                onClick={() => clipboard.copy(runDetails.logs_url)}
              />
              <IosRow
                title="Share link"
                accessory={<IconShare2 size={20} aria-hidden />}
                onClick={(event) => handleShare(runDetails.logs_url, event.currentTarget)}
              />
            </IosGroup>
          )}
          <VisuallyHidden role="status">{clipboard.copied ? 'Link copied' : ''}</VisuallyHidden>
          {showRerun && (
            <IosGroup footer={note}>
              <IosRow
                title={
                  <Text component="span" inherit c="blue">
                    {isFlux ? labels.trigger : 'Rerun workflow'}
                  </Text>
                }
                accessory={rerunPending ? <Loader size="sm" /> : undefined}
                disabled={rerunPending}
                onClick={handleRerun}
              />
            </IosGroup>
          )}
          {stoppable && (
            <IosGroup footer={note}>
              <IosRow
                title="Stop workflow"
                destructive
                accessory={cancelling ? <Loader size="sm" /> : undefined}
                disabled={isPending || cancelling}
                onClick={handleCancel}
              />
            </IosGroup>
          )}
        </Stack>
      )
    }

    return (
      <Stack gap={isMobile ? 'xs' : 'md'}>
        {notices}
        <Paper withBorder p={isMobile ? 'md' : 'lg'}>
          <Stack gap="sm">
            {rows.map(({ field, value }, index) => (
              <Fragment key={field.id}>
                {index > 0 && <Divider />}
                <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                  <Text size={TYPOGRAPHY.FIELD_VALUE.size} c="dimmed" miw={isMobile ? 90 : 130}>
                    {field.label}
                  </Text>
                  <Box flex={1} miw={0}>
                    <Group justify="flex-end" ta="right">
                      {renderValue(field, value)}
                    </Group>
                  </Box>
                </Group>
              </Fragment>
            ))}
          </Stack>
        </Paper>
      </Stack>
    )
  }

  const title = isFlux ? 'Reconciliation details' : 'Run details'

  if (canViewLogs && !mobileShell) {
    const column = { display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 } as const

    return (
      <StandardModal
        opened={opened}
        onClose={onClose}
        size="full"
        title={title}
        footer={footer}
        disableScrollArea
        contentPadding={false}
      >
        <Tabs
          value={tab}
          onChange={(value) => setTab(value === 'details' ? 'details' : 'logs')}
          keepMounted={false}
          onKeyDown={tab === 'logs' ? shortcuts : undefined}
          style={column}
        >
          <Tabs.List px="lg">
            <Tabs.Tab value="logs">Logs</Tabs.Tab>
            <Tabs.Tab value="details">Details</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel value="logs" style={column}>
            <RunLogsPanel
              pipelineId={pipelineId}
              pipelineName={pipelineName}
              runNumber={runNumber}
              providerName={providerName}
              narrow={isMobile}
              viewerRef={viewerRef}
            />
          </Tabs.Panel>
          <Tabs.Panel value="details" style={column}>
            <ScrollArea style={{ flex: 1, minHeight: 0 }}>
              <Box p="lg">{renderContent()}</Box>
            </ScrollArea>
          </Tabs.Panel>
        </Tabs>
      </StandardModal>
    )
  }

  return (
    <>
      <StandardModal
        opened={opened}
        onClose={onClose}
        size="lg"
        title={title}
        footer={mobileShell ? undefined : footer}
        fullHeight
      >
        {renderContent()}
      </StandardModal>
      {mobileShell && logTree && logNode && (
        <StandardModal
          opened
          onClose={() => setLogNodeId(null)}
          title={logNode.name}
          backAction={{ label: `#${runNumber}`, onClick: () => setLogNodeId(null) }}
          disableScrollArea
          contentPadding={false}
          fullHeight
        >
          <LogViewer
            pipelineId={pipelineId}
            pipelineName={pipelineName}
            runNumber={runNumber}
            node={logNode}
            steps={logTree.nodes.filter((n) => n.parent_id === logNode.id && n.kind === 'step')}
            variant="phone"
            providerName={providerName}
          />
        </StandardModal>
      )}
    </>
  )
}
