import 'mantine-datatable/styles.layer.css'

import { Box } from '@mantine/core'
import { DataTable, type DataTableProps, type DataTableRowClickHandler } from 'mantine-datatable'
import type { ReactNode } from 'react'

import { rowClassName, TABLE_HEADER_STYLE, TABLE_NUMERIC } from './tableStyles'

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
        rowClassName={rowClassName(Boolean(props.onRowClick)) ?? ''}
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
            ...TABLE_HEADER_STYLE,
          },
          table: {
            ...(props.styles?.table || {}),
            borderSpacing: '0',
            tableLayout: 'auto',
            width: '100%',
            fontSize: 'var(--mantine-font-size-sm)',
            ...TABLE_NUMERIC,
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
