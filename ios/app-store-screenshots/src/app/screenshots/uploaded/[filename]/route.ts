import type { NextResponse } from 'next/server'
import { serveAsset } from '@/lib/serve-asset'

export const dynamic = 'force-dynamic'

export async function GET(_req: Request, context: { params: Promise<{ filename: string }> }): Promise<NextResponse> {
  const { filename } = await context.params
  return serveAsset('screenshots/uploaded', filename, { png: 'image/png', jpg: 'image/jpeg' })
}
