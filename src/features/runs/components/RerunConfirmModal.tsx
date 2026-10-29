import { Button, Group, Radio, Stack, Text } from '@mantine/core'
import { IconCheck } from '@tabler/icons-react'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { StandardModal } from '../../../components/common/StandardModal'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import type { Pipeline, PipelineRun } from '../../../types/domain'
import { errorMessage } from '../../../utils/errorDisplay'
import {
  buildRerunSummary,
  canRerunFailedOnly,
  type RerunMode,
  type RerunSummaryField,
} from '../../../utils/rerunSummary'

interface RerunConfirmModalProps {
  run: PipelineRun | null
  pipeline: Pipeline | undefined
  providerName: string
  onClose: () => void
  onConfirm: (mode: RerunMode) => Promise<unknown>
}

const MODE_COPY: Record<RerunMode, { label: string; confirm: string; description: string }> = {
  failed: {
    label: 'Re-run failed jobs only',
    confirm: 'Re-run failed jobs',
    description: 'Runs the failed jobs again, plus the jobs that depend on them.',
  },
  all: { label: 'Re-run all jobs', confirm: 'Re-run all jobs', description: 'Runs every job of this run again.' },
}

const SummaryRows = ({ rows }: { rows: RerunSummaryField[] }) => (
  <Stack gap="xs">
    {rows.map(({ label, value }) => (
      <Group key={label} justify="space-between" align="flex-start" wrap="nowrap" gap="md">
        <Text size="sm" c="dimmed">
          {label}
        </Text>
        <Text size="sm" ta="right" style={{ overflowWrap: 'anywhere' }}>
          {value}
        </Text>
      </Group>
    ))}
  </Stack>
)

const ShellRows = ({ rows, header }: { rows: RerunSummaryField[]; header?: string }) => (
  <IosGroup header={header}>
    {rows.map(({ label, value }) =>
      value.length > 32 ? (
        <IosRow key={label} title={value} subtitle={label} />
      ) : (
        <IosRow key={label} title={label} value={value} />
      )
    )}
  </IosGroup>
)

interface RerunActionState {
  error: string | null
}

export const RerunConfirmModal = ({
  run,
  pipeline,
  providerName,
  onClose,
  onConfirm,
}: RerunConfirmModalProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const [choice, setChoice] = useState<RerunMode | null>(null)

  const failedOffered = run !== null && canRerunFailedOnly(run)
  const mode: RerunMode = choice ?? (failedOffered ? 'failed' : 'all')
  const summary = run ? buildRerunSummary(run, { pipeline, providerName }) : { fields: [], inputs: [] }
  const modes: RerunMode[] = failedOffered ? ['failed', 'all'] : []

  const close = () => {
    setChoice(null)
    onClose()
  }

  const [state, formAction, pending] = useActionState(
    async (): Promise<RerunActionState> => {
      try {
        await onConfirm(mode)
      } catch (error: unknown) {
        return { error: errorMessage(error, 'Try the rerun again.') }
      }

      close()

      return { error: null }
    },
    { error: null }
  )

  const footer = (requestClose: () => void) => (
    <Group justify="flex-end" gap="sm" grow={isMobile}>
      <Button variant="default" size="sm" onClick={requestClose} disabled={pending}>
        Cancel
      </Button>
      <Button size="sm" type="submit" form={formId} loading={pending}>
        {MODE_COPY[mode].confirm}
      </Button>
    </Group>
  )

  const shellBody = (
    <Stack gap="lg">
      <ShellRows rows={summary.fields} />
      {summary.inputs.length > 0 && <ShellRows rows={summary.inputs} header="Inputs" />}
      <IosGroup footer={modes.length === 0 ? MODE_COPY.all.description : MODE_COPY[mode].description}>
        {modes.map((value) => (
          <IosRow
            key={value}
            title={MODE_COPY[value].label}
            selected={mode === value}
            accessory={mode === value ? <IconCheck size={18} color="var(--ios-tint)" aria-hidden /> : undefined}
            disabled={pending}
            onClick={() => setChoice(value)}
          />
        ))}
      </IosGroup>
    </Stack>
  )

  const desktopBody = (
    <Stack gap="md">
      <SummaryRows rows={summary.fields} />
      {summary.inputs.length > 0 && (
        <Stack gap="xs">
          <Text size="sm" fw={500}>
            Inputs
          </Text>
          <SummaryRows rows={summary.inputs} />
        </Stack>
      )}
      {modes.length > 0 ? (
        <Radio.Group<RerunMode> name="mode" label="Jobs to rerun" value={mode} onChange={(value) => setChoice(value)}>
          <Stack gap="xs" mt="xs">
            {modes.map((value) => (
              <Radio
                key={value}
                value={value}
                label={MODE_COPY[value].label}
                description={MODE_COPY[value].description}
                disabled={pending}
              />
            ))}
          </Stack>
        </Radio.Group>
      ) : (
        <Text size="sm" c="dimmed">
          {MODE_COPY.all.description}
        </Text>
      )}
    </Stack>
  )

  return (
    <StandardModal
      opened={run !== null}
      onClose={close}
      size="md"
      title={mobileShell ? 'Confirm re-run' : `Re-run: ${pipeline?.name ?? ''}`}
      loading={pending}
      footer={footer}
      primaryAction={{ label: 'Re-run', onClick: () => formRef.current?.requestSubmit(), loading: pending }}
    >
      <form ref={formRef} id={formId} action={formAction}>
        <Stack gap="md">
          {mobileShell ? shellBody : desktopBody}
          <FormErrorDisplay title="Couldn't re-run" globalError={state.error} />
        </Stack>
      </form>
    </StandardModal>
  )
}
