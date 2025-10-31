import { DataTable, DataTableProps } from 'mantine-datatable'

import { Box } from '@mantine/core'


export function StandardTable<T>(props: DataTableProps<T>) {
  return (
    <Box mt="xl">
      <DataTable<T>
        {...props}
        withTableBorder={false}
        highlightOnHover={true}
        scrollAreaProps={{
          type: 'auto',
          scrollbarSize: 10,
          scrollHideDelay: 0,
        }}
        styles={{
          ...props.styles,
          header: {
            fontSize: 'var(--mantine-font-size-md)',
            fontWeight: 600,
            paddingTop: '0.875rem',
            paddingBottom: '0.875rem',
            paddingLeft: '1rem',
            paddingRight: '1rem',
            whiteSpace: 'nowrap',
            verticalAlign: 'middle',
            textAlign: 'left',
            ...(props.styles?.header || {}),
          },
          table: {
            borderSpacing: '0',
            tableLayout: 'fixed',
            width: '100%',
            fontSize: 'var(--mantine-font-size-sm)',
            ...(props.styles?.table || {}),
          },
          pagination: {
            paddingTop: '1rem',
            paddingBottom: '1rem',
            marginBottom: '1.5rem',
            fontSize: 'var(--mantine-font-size-sm)',
            ...(props.styles?.pagination || {}),
          },
        }}
        rowStyle={() => ({
          height: '56px',
          verticalAlign: 'middle',
        })}
        height={props.height || 'calc(100vh - 230px)'}
        verticalSpacing="sm"
        horizontalSpacing="lg"
      />
    </Box>
  )
}
