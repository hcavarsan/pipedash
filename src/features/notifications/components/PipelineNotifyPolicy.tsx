import { Checkbox, Radio, Stack } from '@mantine/core'
import { modals } from '@mantine/modals'
import { type ReactNode, useState } from 'react'

import { Z } from '../../../theme/zIndex'
import type { PipelineNotifyState } from '../../../types/notification_preferences'
import { type NotifyItem, type PolicyTarget, useNotifyItems } from '../hooks/useNotifyItems'

interface PipelineNotifyPolicyProps {
  pipeline: PolicyTarget
  onDone: () => void
}

export function PipelineNotifyPolicy({ pipeline, onDone }: PipelineNotifyPolicyProps): ReactNode {
  const { policyOptions } = useNotifyItems()
  const options = policyOptions(pipeline)
  const [saving, setSaving] = useState<PipelineNotifyState | null>(null)
  const current = saving ?? options.find((option) => option.selected)?.value ?? 'default'

  const choose = async (value: string): Promise<void> => {
    const option = options.find((candidate) => candidate.value === value)

    if (!option || saving) {
      return
    }

    setSaving(option.value)

    if (await option.select()) {
      onDone()
    } else {
      setSaving(null)
    }
  }

  return (
    <Stack gap="lg">
      <Radio.Group value={current} onChange={(value) => void choose(value)}>
        <Stack gap="sm">
          {options.map((option) => (
            <Radio
              key={option.value}
              value={option.value}
              label={option.label}
              description={option.description}
              disabled={saving !== null}
            />
          ))}
        </Stack>
      </Radio.Group>
      <QuietAlertToggle pipeline={pipeline} />
    </Stack>
  )
}

function QuietAlertToggle({ pipeline }: { pipeline: PolicyTarget }): ReactNode {
  const { quietItem } = useNotifyItems()
  const quiet = quietItem(pipeline)
  const [saving, setSaving] = useState(false)

  const toggle = async (): Promise<void> => {
    setSaving(true)
    await quiet.toggle()
    setSaving(false)
  }

  return (
    <Checkbox
      label={quiet.label}
      description={quiet.description}
      checked={quiet.enabled}
      disabled={saving || !quiet.available}
      onChange={() => void toggle()}
    />
  )
}

export function usePipelineNotifyItem(): (pipeline: PolicyTarget) => NotifyItem {
  const { policyOptions } = useNotifyItems()

  return (pipeline) => ({
    label: 'Notifications…',
    icon: policyOptions(pipeline).find((option) => option.selected)?.icon,
    onClick: () => {
      const id = modals.open({
        title: `Notifications for ${pipeline.name}`,
        centered: true,
        zIndex: Z.alert,
        children: <PipelineNotifyPolicy pipeline={pipeline} onDone={() => modals.close(id)} />,
      })
    },
  })
}
