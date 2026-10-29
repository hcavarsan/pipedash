export type MetricsConfigScope = { scope: 'global' } | { scope: 'pipeline'; pipelineId: string; pipelineName: string }

export type MetricsConfigModalProps = MetricsConfigScope & {
  opened: boolean
  onClose: () => void
  onConfigChange?: (() => void) | undefined
}

export interface ConfigSnapshot {
  enabled: boolean
  retentionDays: number
}

export function metricsConfigTitle(scope: MetricsConfigScope, mobileShell: boolean): string {
  if (mobileShell) {
    return 'Configure metrics'
  }

  return scope.scope === 'pipeline' ? `Configure metrics: ${scope.pipelineName}` : 'Global metrics configuration'
}
