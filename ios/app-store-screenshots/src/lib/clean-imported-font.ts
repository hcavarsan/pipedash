import type { ImportedFont } from "./types";

const FORMATS = ["woff2", "woff", "truetype", "opentype"] as const;

// Display names come from the uploaded file name; keep them short and printable.
export function cleanFontName(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const name = value.replace(/[\u0000-\u001f\u007f]/g, "").replace(/\s+/g, " ").trim().slice(0, 60);
  return name || undefined;
}

export function cleanImportedFont(value: unknown): ImportedFont | undefined {
  if (!value || typeof value !== "object") return undefined;
  const font = value as Partial<ImportedFont>;
  if (typeof font.src !== "string" || !/^\/fonts\/imported\/[a-z0-9]+\.(woff2|woff|ttf|otf)$/i.test(font.src)) return undefined;
  if (!FORMATS.includes(font.format as ImportedFont["format"])) return undefined;
  const name = cleanFontName(font.name);
  return { src: font.src, format: font.format as ImportedFont["format"], ...(name ? { name } : {}) };
}
