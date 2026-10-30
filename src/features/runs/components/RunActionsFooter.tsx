import { Button, Group, Loader, Stack, Text, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCopy, IconExternalLink, IconPlayerPlay, IconShare2, IconSquare } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { mobileService } from '../../../services/mobile'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineRun } from '../../../types/domain'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isHttpUrl, openExternal } from '../../../utils/openExternal'
import { isMobilePlatform } from '../../../utils/platform'

interface RunActionsProps {
  run: PipelineRun
  providerName: string
  isMobile: boolean
  isFlux: boolean
  triggerLabel: string
  canViewLogs: boolean
  canTrigger: boolean
  canCancel: boolean
  triggerBlockedReason: string | undefined
  stopDisabled: boolean
  rerunPending: boolean
  cancelPending: boolean
  onRerun: () => void
  onCancel: () => void
}

const FLUX_NOTE = 'Reconcile uses the current resource configuration. It does not restore this historical revision.'
const PENDING_NOTE = 'This run is pending. It can be stopped or rerun once it starts.'

export function RunActionsFooter({
  run,
  providerName,
  isMobile,
  isFlux,
  triggerLabel,
  canViewLogs,
  canTrigger,
  canCancel,
  triggerBlockedReason,
  stopDisabled,
  rerunPending,
  cancelPending,
  onRerun,
  onCancel,
}: RunActionsProps): ReactNode {
  const isRunning = run.status === 'running' || run.status === 'pending'
  const isPending = run.status === 'pending'

  return (
    <Stack gap="xs">
      {(isFlux || triggerBlockedReason) && (
        <Text size="sm" c="dimmed">
          {triggerBlockedReason ?? FLUX_NOTE}
        </Text>
      )}
      {isPending && canCancel && (
        <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} c="dimmed">
          {PENDING_NOTE}
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
            {canViewLogs ? `Open in ${providerName}` : isMobile ? 'Logs' : 'View full logs'}
          </Button>
        )}
        {isRunning && canCancel && (
          <Button
            color="red"
            size="sm"
            leftSection={<IconSquare size={16} aria-hidden />}
            onClick={onCancel}
            loading={cancelPending}
            disabled={isPending || stopDisabled}
          >
            {isMobile ? 'Stop' : 'Stop workflow'}
          </Button>
        )}
        {!isRunning && canTrigger && (
          <Button
            size="sm"
            leftSection={<IconPlayerPlay size={16} aria-hidden />}
            onClick={onRerun}
            loading={rerunPending}
          >
            {isFlux ? triggerLabel : isMobile ? 'Rerun' : 'Rerun workflow'}
          </Button>
        )}
      </Group>
    </Stack>
  )
}

export function MobileRunActions({
  run,
  isFlux,
  triggerLabel,
  canTrigger,
  canCancel,
  triggerBlockedReason,
  rerunPending,
  cancelPending,
  onRerun,
  onCancel,
}: RunActionsProps): ReactNode {
  const clipboard = useClipboard({ timeout: 2000 })
  const isRunning = run.status === 'running' || run.status === 'pending'
  const isPending = run.status === 'pending'
  const hasLogs = isHttpUrl(run.logs_url)
  const stoppable = isRunning && canCancel
  const showRerun = !isRunning && canTrigger
  const note =
    isFlux || triggerBlockedReason
      ? (triggerBlockedReason ?? FLUX_NOTE)
      : isPending && canCancel
        ? PENDING_NOTE
        : undefined

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
            title={
              <Text component="span" inherit c="blue">
                {isFlux ? triggerLabel : 'Rerun workflow'}
              </Text>
            }
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
            disabled={isPending || cancelPending}
            onClick={onCancel}
          />
        </IosGroup>
      )}
    </>
  )
}
