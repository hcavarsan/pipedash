import type { ReactNode } from 'react'
import { Funnel, FunnelChart, LabelList, ResponsiveContainer, Tooltip } from 'recharts'

import type { FunnelStep } from '../../../types/applications'

export function FunnelPlot({ steps }: { steps: FunnelStep[] }): ReactNode {
  const data = steps.map((step) => ({ name: step.label, value: step.count, fill: 'var(--mantine-color-blue-filled)' }))

  return (
    <ResponsiveContainer width="100%" height={240}>
      <FunnelChart>
        <Tooltip />
        <Funnel dataKey="value" data={data} isAnimationActive={false}>
          <LabelList position="right" dataKey="name" fill="var(--mantine-color-text)" stroke="none" />
        </Funnel>
      </FunnelChart>
    </ResponsiveContainer>
  )
}
