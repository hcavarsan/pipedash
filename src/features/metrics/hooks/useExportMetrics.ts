import { type UseMutationResult, useMutation } from '@tanstack/react-query'
import type { RefObject } from 'react'
import type { MetricsDashboard, MetricType } from '../../../types/metrics'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'
import { formatDateTime } from '../../../utils/formatDateTime'
import { saveFile } from '../../../utils/saveFile'

type ExportFormat = 'png' | 'csv'

const VALUE_UNITS: Record<MetricType, string> = {
  run_duration: 'seconds',
  success_rate: '%',
  run_frequency: 'runs',
}

interface ExportSource {
  data: MetricsDashboard | undefined
  metricType: MetricType
  fileBaseName: string
  chartRef: RefObject<HTMLDivElement | null>
}

async function buildPngContent(chart: HTMLDivElement | null): Promise<Uint8Array<ArrayBuffer>> {
  if (!chart) {
    throw new Error('The chart is not ready to export.')
  }
  const { toBlob } = await import('html-to-image')
  const backgroundColor = window.getComputedStyle(document.body).backgroundColor
  const image = await toBlob(chart, { pixelRatio: 2, backgroundColor })

  if (!image) {
    throw new Error('The chart image could not be created.')
  }
  return new Uint8Array(await image.arrayBuffer())
}

function buildCsvContent({ data, observed }: MetricsDashboard, metricType: MetricType): string {
  const cell = (value: number | string | null) => `"${value ?? ''}"`
  const headers = ['Timestamp', 'Date', `Value (${VALUE_UNITS[metricType]})`, 'Count', 'Min', 'Max', 'Avg']
  const rows = data.metrics.map((metric, index) => [
    metric.timestamp,
    formatDateTime(metric.timestamp),
    observed[index] ? metric.value : null,
    metric.count,
    metric.min,
    metric.max,
    observed[index] ? metric.avg : null,
  ])

  return [headers, ...rows].map((row) => row.map(cell).join(',')).join('\n')
}

export function useExportMetrics({
  data,
  metricType,
  fileBaseName,
  chartRef,
}: ExportSource): UseMutationResult<boolean, Error, ExportFormat> {
  return useMutation({
    mutationFn: async (format: ExportFormat) => {
      if (!data) {
        return false
      }

      const content = format === 'png' ? await buildPngContent(chartRef.current) : buildCsvContent(data, metricType)

      return saveFile(`${fileBaseName}.${format}`, content, {
        name: format === 'png' ? 'PNG Image' : 'CSV File',
        extension: format,
        mimeType: format === 'png' ? 'image/png' : 'text/csv',
      })
    },
    onSuccess: (saved, format) => {
      if (saved) {
        displaySuccessNotification(
          format === 'png' ? 'Chart exported as PNG' : 'Data exported as CSV',
          format === 'png' ? 'Chart exported' : 'Data exported'
        )
      }
    },
    onError: (error, format) => {
      displayErrorNotification(error, `Couldn't export ${format.toUpperCase()}`)
    },
  })
}
