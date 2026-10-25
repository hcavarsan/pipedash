import { randomUUID } from "node:crypto";
import { promises as fs } from "node:fs";

/** A concurrent upload/GET must never see a partially written asset. */
export async function writeAsset(filename: string, bytes: Buffer) {
  const temporary = `${filename}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(temporary, bytes);
    await fs.rename(temporary, filename);
  } finally {
    await fs.unlink(temporary).catch(() => undefined);
  }
}
