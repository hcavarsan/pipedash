import 'mantine-datatable/styles.layer.css'

import { Box } from '@mantine/core'
import { DataTable, type DataTableProps, type DataTableRowClickHandler } from 'mantine-datatable'
import type { ReactNode } from 'react'

const INTERACTIVE_TARGET = 'button, a, input, label, [role="checkbox"]'

export function StandardTable<T>(props: DataTableProps<T>): ReactNode {
  const handleRowClick: DataTableRowClickHandler<T> = (params) => {
    const target = params.event.target

    if (!(target instanceof Element)) {
      return
    }
    if (!params.event.currentTarget.contains(target)) {
      return
    }
    if (target.closest(INTERACTIVE_TARGET)) {
      return
    }
    props.onRowClick?.(params)
  }

  return (
    <Box
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        minHeight: 0,
        flex: 1,
        position: 'relative',
      }}
    >
      <DataTable<T>
        {...props}
        withTableBorder={false}
        rowClassName={props.onRowClick ? 'interactive-surface row-clickable' : 'interactive-surface'}
        {...(props.onRowClick ? { onRowClick: handleRowClick } : {})}
        withColumnBorders={false}
        scrollAreaProps={{
          type: 'auto',
          scrollbarSize: 10,
          scrollHideDelay: 0,
        }}
        styles={{
          ...props.styles,
          header: {
            ...(props.styles?.header || {}),
            fontSize: 'var(--mantine-font-size-sm)',
            fontWeight: 600,
            paddingTop: 'var(--mantine-spacing-sm)',
            paddingBottom: 'var(--mantine-spacing-sm)',
            paddingLeft: 'var(--mantine-spacing-md)',
            paddingRight: 'var(--mantine-spacing-md)',
            whiteSpace: 'nowrap',
            verticalAlign: 'middle',
            borderBottom: '1px solid var(--mantine-color-default-border)',
          },
          table: {
            ...(props.styles?.table || {}),
            borderSpacing: '0',
            tableLayout: 'auto',
            width: '100%',
            fontSize: 'var(--mantine-font-size-sm)',
            fontVariantNumeric: 'tabular-nums',
          },
          pagination: {
            ...(props.styles?.pagination || {}),
            paddingTop: 'var(--mantine-spacing-xs)',
            fontSize: 'var(--mantine-font-size-sm)',
          },
        }}
        rowStyle={(record, index) => [{ height: '48px', verticalAlign: 'middle' }, props.rowStyle?.(record, index)]}
        height={props.height || '100%'}
        verticalSpacing="xs"
        horizontalSpacing="md"
      />
    </Box>
  )
}
