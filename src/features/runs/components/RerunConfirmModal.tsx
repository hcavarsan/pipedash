import { Button, Group, Radio, Stack, Text } from '@mantine/core'
import { IconCheck } from '@tabler/icons-react'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { StandardModal } from '../../../components/common/StandardModal'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import type { Pipeline, PipelineRun } from '../../../types/pipelines'
import type { DisplayField } from '../../../types/presentation'
import { errorMessage } from '../../../utils/errorDisplay'
import { sameJson } from '../../../utils/sameJson'
import type { RerunMode } from '../lib/runActions'
import { usePreparedTrigger } from '../queries/useWorkflowQueries'

interface RerunConfirmModalProps {
  run: PipelineRun | null
  pipeline: Pipeline | undefined
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

const SummaryRows = ({ rows }: { rows: DisplayField[] }) => (
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

const ShellRows = ({ rows }: { rows: DisplayField[] }) => (
  <IosGroup>
    {rows.map(({ label, value }) =>
      value.length > 32 ? (
        <IosRow key={label} title={value} subtitle={label} />
      ) : (
        <IosRow key={label} title={label} value={value} />
      )
    )}
  </IosGroup>
)

interface ModeRadiosProps {
  modes: RerunMode[]
  mode: RerunMode
  pending: boolean
  onChange: (mode: RerunMode) => void
}

const ModeRadios = ({ modes, mode, pending, onChange }: ModeRadiosProps): ReactNode => (
  <Radio.Group<RerunMode> name="mode" label="Jobs to rerun" value={mode} onChange={onChange}>
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
)

interface RerunActionState {
  error: string | null
  submitted: { mode: RerunMode; runId: string } | null
}

export const RerunConfirmModal = ({ run, pipeline, onClose, onConfirm }: RerunConfirmModalProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const [choice, setChoice] = useState<RerunMode | null>(null)

  const { data: draft } = usePreparedTrigger(pipeline?.id ?? '', run?.run_number, run !== null)
  const failedOffered = run?.actions.rerun_failed.allowed === true
  const mode: RerunMode = choice ?? (failedOffered ? 'failed' : 'all')
  const fields = draft?.fields ?? []
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
        return {
          error: errorMessage(error, 'Try the rerun again.'),
          submitted: { mode, runId: run?.id ?? '' },
        }
      }

      close()

      return { error: null, submitted: null }
    },
    { error: null, submitted: null }
  )
  const submittedError = sameJson(state.submitted, { mode, runId: run?.id ?? '' }) ? state.error : null

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
      <ShellRows rows={fields} />
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
      <SummaryRows rows={fields} />
      {modes.length > 0 ? (
        <ModeRadios modes={modes} mode={mode} pending={pending} onChange={setChoice} />
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
          <FormErrorDisplay title="Couldn't re-run" globalError={submittedError} />
        </Stack>
      </form>
    </StandardModal>
  )
}
