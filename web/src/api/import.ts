import type { components } from "./schema";
import { ApiError } from "./problem";
import { apiFetch } from "./client";

export type ImportRequest = components["schemas"]["ImportRequest"];
export type ImportReport = components["schemas"]["ImportReport"];
export type ImportResult = components["schemas"]["ImportResult"];
export type ImportRowError = components["schemas"]["ImportRowError"];

/** 本登録の結果。422(ImportReport) は例外ではなくこの形で返す */
export type ImportOutcome =
  | { kind: "inserted"; result: ImportResult }
  | { kind: "rejected"; report: ImportReport };

export function isImportReport(value: unknown): value is ImportReport {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.total_rows === "number" &&
    typeof v.to_insert === "number" &&
    typeof v.to_skip_duplicate === "number" &&
    Array.isArray(v.errors)
  );
}

export function hasErrors(report: ImportReport): boolean {
  return report.errors.length > 0;
}

export const CSV_MAX_BYTES = 1024 * 1024; // 1MiB
export const CSV_MAX_ROWS = 5000;

export type CsvEncoding = "utf-8" | "shift_jis";

export class CsvInputError extends Error {}

export async function readCsvFile(file: File, encoding: CsvEncoding): Promise<string> {
  if (file.size > CSV_MAX_BYTES) {
    throw new CsvInputError(
      `ファイルサイズが上限(${Math.floor(CSV_MAX_BYTES / 1024)}KB)を超えています`,
    );
  }
  const buffer = await file.arrayBuffer();
  // utf-8 は既定でBOMを除去する。shift_jis にBOMは無い
  return new TextDecoder(encoding).decode(buffer);
}

/** ヘッダ行を除いたデータ行数。空行は数えない */
export function countDataRows(csv: string): number {
  const lines = csv.split(/\r?\n/).filter((line) => line.trim() !== "");
  return Math.max(lines.length - 1, 0);
}

export const CSV_TEMPLATE_HEADER =
  "account,symbol,kind,quantity,price,fee,traded_at,note,external_id";

  /** 422 の本文が ImportReport なら握り、それ以外の ApiError は投げ直す */
function unwrapReport(err: unknown): ImportReport | null {
  if (!(err instanceof ApiError)) return null;
  if (err.status !== 422) return null;
  return isImportReport(err.problem) ? err.problem : null;
}

/** 検証のみ。エラーがあっても 200 で ImportReport が返る */
export async function dryRunImport(csvContent: string): Promise<ImportReport> {
  const body: ImportRequest = { csv_content: csvContent };
  try {
    return await apiFetch<ImportReport>("/import/transactions/dry-run", {
      method: "POST",
      body,
    });
  } catch (err) {
    // 仕様上ここには来ないが、来たなら検証結果として扱えるので握る
    const report = unwrapReport(err);
    if (report) return report;
    throw err;
  }
}

/** 本登録。422(ImportReport) は例外にせず rejected として返す */
export async function runImport(csvContent: string): Promise<ImportOutcome> {
  const body: ImportRequest = { csv_content: csvContent };
  try {
    const result = await apiFetch<ImportResult>("/import/transactions", {
      method: "POST",
      body,
    });
    return { kind: "inserted", result };
  } catch (err) {
    const report = unwrapReport(err);
    if (report) return { kind: "rejected", report };
    throw err; // 401・500・通信エラーは従来どおり
  }
}

export type ImportCounts = {
  inserted: number;
  skippedDuplicate: number;
};

export function countsFromResult(result: ImportResult): ImportCounts {
  return { inserted: result.inserted, skippedDuplicate: result.skipped_duplicate };
}
