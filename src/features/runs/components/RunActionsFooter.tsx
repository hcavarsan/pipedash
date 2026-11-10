import { Button, Group, Loader, Stack, Text, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCopy, IconExternalLink, IconShare2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { mobileService } from '../../../services/mobile'
import type { PipelineRun } from '../../../types/pipelines'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isHttpUrl, openLink } from '../../../utils/openExternal'
import { isMobilePlatform } from '../../../utils/platform'
import type { RunAction } from '../lib/runActions'

interface RunActionsProps {
  run: PipelineRun
  providerName: string
  isMobile: boolean
  canViewLogs: boolean
  note: string | null | undefined
  actions: RunAction[]
}

const VARIANTS: Record<string, 'outline' | 'default'> = { 'force-cancel': 'outline', watch: 'default' }

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
  canViewLogs,
  note,
  actions,
}: RunActionsProps): ReactNode {
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
        {actions.map((action) => (
          <Button
            key={action.id}
            size="sm"
            variant={VARIANTS[action.id] ?? 'filled'}
            {...(action.color ? { color: action.color } : {})}
            leftSection={action.icon}
            loading={action.pending}
            disabled={action.disabled}
            onClick={() => void action.execute()}
          >
            {action.label}
          </Button>
        ))}
      </Group>
    </Stack>
  )
}

const RerunTitle = ({ label }: { label: string }): ReactNode => (
  <Text component="span" inherit c="blue">
    {label}
  </Text>
)

export function MobileRunActions({ run, providerName, note, actions }: RunActionsProps): ReactNode {
  const clipboard = useClipboard({ timeout: 2000 })
  const hasLogs = isHttpUrl(run.logs_url)

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
        <IosGroup footer={actions.length === 0 ? note : undefined}>
          <IosRow
            title={`Open in ${providerName}`}
            accessory={<IconExternalLink size={20} aria-hidden />}
            onClick={() => openLink(run.logs_url)}
          />
          <IosRow
            title={clipboard.copied ? 'Copied' : 'Copy link'}
            accessory={<IconCopy size={20} aria-hidden />}
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
      {actions.length > 0 && (
        <IosGroup footer={note}>
          {actions.map((action) => (
            <IosRow
              key={action.id}
              title={action.id === 'rerun' ? <RerunTitle label={action.label} /> : action.label}
              destructive={action.color === 'red'}
              accessory={action.pending ? <Loader size="xs" /> : undefined}
              disabled={action.disabled}
              onClick={() => void action.execute()}
            />
          ))}
        </IosGroup>
      )}
    </>
  )
}
