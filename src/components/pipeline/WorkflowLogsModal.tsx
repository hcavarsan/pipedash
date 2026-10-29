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
import { Fragment, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { usePlugins } from '../../queries/usePluginsQueries'
import { useRerunWorkflow, useRunDetails } from '../../queries/useRunDetailsQuery'
import { useCancelRun, useRerunFailedJobs, useRerunRun } from '../../queries/useRunHistoryQueries'
import { useRunLogTree } from '../../queries/useRunLogQueries'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import { mobileService } from '../../services/mobile'
import { STATUS_META } from '../../theme/status'
import { TYPOGRAPHY } from '../../theme/typography'
import type { CellRenderer, Pipeline, PipelineStatus } from '../../types'
import { filterVisibleColumns } from '../../utils/columnBuilder'
import { isCellValue, renderCellValue } from '../../utils/dynamicRenderers'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatDuration } from '../../utils/formatDuration'
import { getValueByPath } from '../../utils/objectPath'
import { isHttpUrl, openExternal } from '../../utils/openExternal'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { isMobilePlatform } from '../../utils/platform'
import { getPluginDisplayName } from '../../utils/pluginHelpers'
import type { RerunMode } from '../../utils/rerunSummary'
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
import { RerunConfirmModal } from './RerunConfirmModal'
import { RunLogsPanel } from './RunLogsPanel'
import classes from './WorkflowLogsModal.module.css'

interface WorkflowLogsModalProps {
  opened: boolean
  onClose: () => void
  pipelineId: string
  runNumber: number
  providerId?: number | undefined
  providerType?: string | undefined
  pipeline: Pipeline | undefined
  canViewLogs: boolean
  canTrigger: boolean
  triggerBlockedReason?: string | undefined
  canCancel: boolean
  rerunInPlace?: boolean | undefined
  onRerunSuccess?: ((pipelineId: string, newRunNumber: number) => void) | undefined
  onCancelSuccess?: (() => void) | undefined
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
  pipeline,
  canViewLogs,
}: WorkflowLogsModalProps): ReactNode => {
  const pipelineName = pipeline?.name ?? ''
  const [cancelling, setCancelling] = useState(false)
  const [confirmingRerun, setConfirmingRerun] = useState(false)
  const [cancellationRequestedFor, setCancellationRequestedFor] = useState<string | null>(null)
  const cancelRun = useCancelRun()
  const clipboard = useClipboard({ timeout: 2000 })
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const abortRef = useRef(false)
  const labels = getPipelineLabels(providerType)
  const isFlux = providerType === 'fluxcd'
  const { data: plugins } = usePlugins()
  const providerName = providerType ? getPluginDisplayName(plugins, providerType) : 'provider'
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
  const rerunFailedMutation = useRerunFailedJobs()
  const rerunPending = rerunMutation.isPending || rerunRunMutation.isPending || rerunFailedMutation.isPending

  const { data: tableSchema } = useTableDefinition(providerId ?? 0, 'pipeline_runs')

  const fields = useMemo<DetailField[]>(() => {
    const columns = tableSchema ? filterVisibleColumns(tableSchema.columns).filter((col) => col.id !== 'actions') : []

    return columns.length > 0 ? columns : FALLBACK_FIELDS
  }, [tableSchema])

  const isRunning = runDetails?.status === 'running' || runDetails?.status === 'pending'
  const isPending = runDetails?.status === 'pending'

  const handleRerun = async (mode: RerunMode) => {
    if (!runDetails) {
      return
    }

    try {
      if (mode === 'failed' || rerunInPlace) {
        await (mode === 'failed' ? rerunFailedMutation : rerunRunMutation).mutateAsync({ pipelineId, runNumber })
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

  const requestRerun = () => (isFlux ? handleRerun('all') : setConfirmingRerun(true))

  const handleCancel = async () => {
    if (!runDetails) {
      return
    }

    if (!mobileShell) {
      setCancelling(true)
      try {
        const requested = await cancelRun.mutateAsync({ pipelineId, runNumber, pipeline, run: runDetails })
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
      if (!(await cancelRun.mutateAsync({ pipelineId, runNumber, pipeline, run: runDetails }))) {
        return
      }

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
    } catch {
      return
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

    return renderCellValue(field.renderer, isCellValue(value) ? value : undefined)
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
          <Button size="sm" leftSection={<IconPlayerPlay size={16} />} onClick={requestRerun} loading={rerunPending}>
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
                ((field.renderer === 'Text' || field.renderer === 'TruncatedText') &&
                  typeof value === 'string' &&
                  value.length > 32) ? (
                  // Long text wraps under its label; a truncated value would hide the rest on touch screens.
                  <IosRow
                    key={field.id}
                    title={field.renderer === 'TruncatedText' ? String(value) : renderValue(field, value)}
                    subtitle={field.label}
                  />
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
                onClick={requestRerun}
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

  const rerunConfirm = (
    <RerunConfirmModal
      run={confirmingRerun && runDetails ? runDetails : null}
      pipeline={pipeline}
      providerName={providerName}
      onClose={() => setConfirmingRerun(false)}
      onConfirm={handleRerun}
    />
  )

  if (canViewLogs && !mobileShell) {
    const column = { display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 } as const

    return (
      <>
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
        {rerunConfirm}
      </>
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
      {rerunConfirm}
    </>
  )
}
