import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { groupOptions } from '../lib/targetGroups'
import type { ApplicationModalProps } from './applicationModalProps'
import { TargetDirectionSection } from './TargetDirectionSection'
import { TargetGroupSection } from './TargetGroupSection'
import { TargetRenameSection } from './TargetRenameSection'

export function TargetsModal({ detail, copy, onClose }: ApplicationModalProps): ReactNode {
  const groups = groupOptions(detail.targets)

  return (
    <StandardModal opened onClose={onClose} size="lg" title={copy.targets}>
      <Stack gap="lg">
        <TargetRenameSection detail={detail} copy={copy} groups={groups} />
        <TargetGroupSection detail={detail} copy={copy} />
        <Stack gap="xs">
          <Text size="sm" fw={500}>
            {copy.direction}
          </Text>
          <TargetDirectionSection detail={detail} copy={copy} groups={groups} />
        </Stack>
      </Stack>
    </StandardModal>
  )
}
