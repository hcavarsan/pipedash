import { Button, Group, Loader, Stack, Text, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCopy, IconExternalLink, IconPlayerPlay, IconShare2, IconSquare } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { mobileService } from '../../../services/mobile'
import type { PipelineRun } from '../../../types/pipelines'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isHttpUrl, openExternal } from '../../../utils/openExternal'
import { isMobilePlatform } from '../../../utils/platform'

interface RunActionsProps {
  run: PipelineRun
  providerName: string
  isMobile: boolean
  rerunLabel: string
  rerunCompactLabel: string
  triggerNote: string | null | undefined
  canViewLogs: boolean
  stopDisabled: boolean
  rerunPending: boolean
  cancelPending: boolean
  forceCancelPending: boolean
  onRerun: () => void
  onCancel: () => void
  onForceCancel: () => void
}

const logsLabel = (canViewLogs: boolean, isMobile: boolean, providerName: string): string => {
  if (canViewLogs) {
    return `Open in ${providerName}`
  }

  return isMobile ? 'Logs' : 'View full logs'
}

export function RunActionsFooter({
  run,
  providerName,
  isMobile,
  rerunLabel,
  rerunCompactLabel,
  triggerNote,
  canViewLogs,
  stopDisabled,
  rerunPending,
  cancelPending,
  forceCancelPending,
  onRerun,
  onCancel,
  onForceCancel,
}: RunActionsProps): ReactNode {
  const isRunning = run.status === 'running' || run.status === 'pending'
  const canCancel = run.actions.cancel.allowed
  const note = run.actions.rerun.reason ?? triggerNote

  return (
    <Stack gap="xs">
      {note && (
        <Text size="sm" c="dimmed">
          {note}
        </Text>
      )}
      <Group justify="flex-end" gap="sm" grow={isMobile}>
        {run.logs_url && (
          <Button
            component="a"
            href={run.logs_url}
            target="_blank"
            rel="noopener noreferrer"
            variant="default"
            size="sm"
            rightSection={<IconExternalLink size={16} aria-hidden />}
          >
            {logsLabel(canViewLogs, isMobile, providerName)}
          </Button>
        )}
        {isRunning && canCancel && (
          <Button
            color="red"
            size="sm"
            leftSection={<IconSquare size={16} aria-hidden />}
            onClick={onCancel}
            loading={cancelPending}
            disabled={stopDisabled}
          >
            {isMobile ? 'Stop' : 'Stop workflow'}
          </Button>
        )}
        {run.actions.force_cancel.allowed && (
          <Button
            color="red"
            variant="outline"
            size="sm"
            onClick={onForceCancel}
            loading={forceCancelPending}
            disabled={cancelPending}
          >
            Force cancel
          </Button>
        )}
        {run.actions.rerun.allowed && (
          <Button
            size="sm"
            leftSection={<IconPlayerPlay size={16} aria-hidden />}
            onClick={onRerun}
            loading={rerunPending}
          >
            {isMobile ? rerunCompactLabel : rerunLabel}
          </Button>
        )}
      </Group>
    </Stack>
  )
}

const RerunTitle = ({ label }: { label: string }): ReactNode => (
  <Text component="span" inherit c="blue">
    {label}
  </Text>
)

export function MobileRunActions({
  run,
  rerunLabel,
  triggerNote,
  stopDisabled,
  rerunPending,
  cancelPending,
  forceCancelPending,
  onRerun,
  onCancel,
  onForceCancel,
}: RunActionsProps): ReactNode {
  const clipboard = useClipboard({ timeout: 2000 })
  const hasLogs = isHttpUrl(run.logs_url)
  const canCancel = run.actions.cancel.allowed
  const stoppable = canCancel
  const showRerun = run.actions.rerun.allowed
  const note = run.actions.rerun.reason ?? triggerNote

  const share = async (source: HTMLElement) => {
    if (isMobilePlatform()) {
      try {
        await mobileService.shareLink(run.logs_url, source.getBoundingClientRect())
      } catch (error) {
        displayErrorNotification(error, "Couldn't share link")
      }

      return
    }

    if (typeof navigator.share !== 'function') {
      clipboard.copy(run.logs_url)

      return
    }

    try {
      await navigator.share({ title: `Run #${run.run_number}`, url: run.logs_url })
    } catch (error: unknown) {
      if (!(error instanceof DOMException && error.name === 'AbortError')) {
        clipboard.copy(run.logs_url)
      }
    }
  }

  return (
    <>
      {hasLogs && (
        <IosGroup footer={showRerun || stoppable ? undefined : note}>
          <IosRow
            title="View logs"
            accessory={<IconExternalLink size={18} aria-hidden />}
            onClick={() => openExternal(run.logs_url)}
          />
          <IosRow
            title="Copy link"
            value={clipboard.copied ? 'Copied' : undefined}
            accessory={<IconCopy size={18} aria-hidden />}
            onClick={() => clipboard.copy(run.logs_url)}
          />
          <IosRow
            title="Share link"
            accessory={<IconShare2 size={20} aria-hidden />}
            onClick={(event) => share(event.currentTarget)}
          />
        </IosGroup>
      )}
      <VisuallyHidden role="status">{clipboard.copied ? 'Link copied' : ''}</VisuallyHidden>
      {showRerun && (
        <IosGroup footer={note}>
          <IosRow
            title={<RerunTitle label={rerunLabel} />}
            accessory={rerunPending ? <Loader size="sm" /> : undefined}
            disabled={rerunPending}
            onClick={onRerun}
          />
        </IosGroup>
      )}
      {stoppable && (
        <IosGroup footer={note}>
          <IosRow
            title="Stop workflow"
            destructive
            accessory={cancelPending ? <Loader size="sm" /> : undefined}
            disabled={cancelPending || stopDisabled}
            onClick={onCancel}
          />
        </IosGroup>
      )}
      {run.actions.force_cancel.allowed && (
        <IosGroup>
          <IosRow
            title="Force cancel"
            destructive
            accessory={forceCancelPending ? <Loader size="sm" /> : undefined}
            disabled={cancelPending || forceCancelPending}
            onClick={onForceCancel}
          />
        </IosGroup>
      )}
    </>
  )
}
