import { Checkbox, Stack } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { confirmBody } from '../../../components/common/ActionTargetRows'
import type { DisplayField } from '../../../types/presentation'

interface CancelRunConfirmProps {
  message: string
  targets: DisplayField[]
  force: { initial: boolean; locked: boolean; help: string | undefined } | null
  onForceChange: (force: boolean) => void
}

export function CancelRunConfirm({ message, targets, force, onForceChange }: CancelRunConfirmProps): ReactNode {
  const [checked, setChecked] = useState(force?.initial ?? false)

  return (
    <Stack gap="md">
      {confirmBody(message, targets)}
      {force && (
        <Checkbox
          label="Force cancel"
          description={force.help}
          checked={checked}
          disabled={force.locked}
          onChange={(event) => {
            setChecked(event.currentTarget.checked)
            onForceChange(event.currentTarget.checked)
          }}
        />
      )}
    </Stack>
  )
}
