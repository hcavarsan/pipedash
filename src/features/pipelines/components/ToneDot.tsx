import { Box } from '@mantine/core'
import type { ReactNode } from 'react'

import { type ProviderTone, TONE_META } from '../../../utils/fetchInfo'

export function ToneDot({ tone }: { tone: ProviderTone }): ReactNode {
  return (
    <Box
      component="span"
      w={8}
      h={8}
      bdrs="50%"
      bg={TONE_META[tone].color}
      style={{ display: 'inline-block', flexShrink: 0 }}
      aria-hidden
    />
  )
}
