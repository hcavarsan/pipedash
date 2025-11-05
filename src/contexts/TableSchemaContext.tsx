import { createContext, useCallback, useContext, useRef, useState } from 'react'

import { invoke } from '@tauri-apps/api/core'

import type { TableDefinition, TableSchema } from '../types'

interface TableSchemaContextType {
  getTableSchema: (providerId: number, tableId: string) => Promise<TableDefinition | null>;
  schemas: Map<number, TableSchema>;
  loading: boolean;
  error: string | null;
}

const TableSchemaContext = createContext<TableSchemaContextType | undefined>(undefined)

export function TableSchemaProvider({ children }: { children: React.ReactNode }) {
  const [schemas, setSchemas] = useState<Map<number, TableSchema>>(new Map())
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const schemasRef = useRef(schemas)
  const pendingRequests = useRef<Map<number, Promise<TableSchema>>>(new Map())

  schemasRef.current = schemas

  const getTableSchema = useCallback(
    async (providerId: number, tableId: string): Promise<TableDefinition | null> => {
      const cachedSchema = schemasRef.current.get(providerId)

      if (cachedSchema) {
        const table = cachedSchema.tables.find((t) => t.id === tableId)


        
return table ?? null
      }

      const pendingRequest = pendingRequests.current.get(providerId)


      if (pendingRequest) {
        try {
          const schema = await pendingRequest
          const table = schema.tables.find((t) => t.id === tableId)


          
return table ?? null
        } catch {
          return null
        }
      }

      const fetchPromise = (async () => {
        try {
          setLoading(true)
          setError(null)

          const schema = await invoke<TableSchema>('get_provider_table_schema', {
            providerId,
          })

          setSchemas((prev) => new Map(prev).set(providerId, schema))

          return schema
        } catch (err: any) {
          const errorMsg = err?.error || err?.message || 'Failed to load table schema'

          console.error('Failed to load table schema:', errorMsg)
          setError(errorMsg)

          throw err
        } finally {
          setLoading(false)
        }
      })()

      pendingRequests.current.set(providerId, fetchPromise)

      try {
        const schema = await fetchPromise
        const table = schema.tables.find((t) => t.id === tableId)


        
return table ?? null
      } catch {
        return null
      } finally {
        pendingRequests.current.delete(providerId)
      }
    },
    []
  )

  return (
    <TableSchemaContext.Provider value={{ getTableSchema, schemas, loading, error }}>
      {children}
    </TableSchemaContext.Provider>
  )
}

export function useTableSchema() {
  const context = useContext(TableSchemaContext)


  if (context === undefined) {
    throw new Error('useTableSchema must be used within a TableSchemaProvider')
  }
  
return context
}
