import { createHash, randomUUID } from 'node:crypto'
import { promises as fs } from 'node:fs'
import path from 'node:path'
import { NextResponse } from 'next/server'
import { projectValidationError } from '@/lib/project-validation'
import { readJsonBody } from '@/lib/request-body'
import { rejectCrossSiteWrite } from '@/lib/request-guard'
import { isMissingFile } from '@/lib/serve-asset'

export const dynamic = 'force-dynamic'

const PROJECT_FILE = 'app-store-screenshots.json'

function filePath(): string {
  return path.join(process.cwd(), PROJECT_FILE)
}

const revision = (raw: string | null): string =>
  `"${raw === null ? 'missing' : createHash('sha256').update(raw).digest('hex')}"`
let writes: Promise<unknown> = Promise.resolve()

export async function GET(): Promise<NextResponse> {
  try {
    const raw = await fs.readFile(filePath(), 'utf8')
    const parsed = JSON.parse(raw)
    const validationError = projectValidationError(parsed)
    if (validationError) {
      throw new Error(validationError)
    }
    return NextResponse.json(
      { ok: true, state: parsed },
      { headers: { ETag: revision(raw), 'Cache-Control': 'no-store' } }
    )
  } catch (e) {
    if (isMissingFile(e)) {
      return NextResponse.json(
        { ok: true, state: null },
        { headers: { ETag: revision(null), 'Cache-Control': 'no-store' } }
      )
    }
    return NextResponse.json({ ok: false, error: e instanceof Error ? e.message : String(e) }, { status: 500 })
  }
}

export async function POST(req: Request): Promise<NextResponse> {
  // This route OVERWRITES a git-tracked file. See lib/request-guard.ts.
  const blocked = rejectCrossSiteWrite(req)
  if (blocked) {
    return NextResponse.json({ ok: false, error: blocked.error }, { status: blocked.status })
  }
  const parsed = await readJsonBody(req, 64 * 1024 * 1024)
  if (parsed.response) {
    return parsed.response
  }
  const body = parsed.value
  const validationError = projectValidationError(body)
  if (validationError) {
    return NextResponse.json({ ok: false, error: validationError }, { status: 400 })
  }
  // Compare and replace in one queue so two tabs cannot both save the same
  // revision. CLI writers may omit If-Match for backwards compatibility.
  const result = writes.then(() => writeProject(body, req.headers.get('if-match')))
  writes = result.catch(() => undefined)
  return result
}

async function writeProject(body: unknown, expected: string | null): Promise<NextResponse> {
  const temporary = `${filePath()}.${randomUUID()}.tmp`
  try {
    if (expected !== null) {
      let current: string | null
      try {
        current = await fs.readFile(filePath(), 'utf8')
      } catch (error) {
        if (!isMissingFile(error)) {
          throw error
        }
        current = null
      }
      if (expected !== revision(current)) {
        return NextResponse.json(
          {
            ok: false,
            error:
              'Project changed in another tab or on disk. Your edits are still open here; export or copy them before reloading.',
          },
          { status: 412 }
        )
      }
    }
    const pretty = `${JSON.stringify(body, null, 2)}\n`
    // Readers must see either the previous complete project or the next one.
    await fs.writeFile(temporary, pretty, 'utf8')
    await fs.rename(temporary, filePath())
    return NextResponse.json({ ok: true }, { headers: { ETag: revision(pretty) } })
  } catch (e) {
    return NextResponse.json({ ok: false, error: e instanceof Error ? e.message : String(e) }, { status: 500 })
  } finally {
    await fs.unlink(temporary).catch(() => undefined)
  }
}
