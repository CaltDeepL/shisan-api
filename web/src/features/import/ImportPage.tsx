import { useRef, useState } from "react";
import { ApiError } from "../../api/problem";
import {
  CSV_MAX_ROWS,
  CSV_TEMPLATE_HEADER,
  CsvInputError,
  countDataRows,
  countsFromResult,
  hasErrors,
  readCsvFile,
  type CsvEncoding,
  type ImportCounts,
  type ImportReport,
} from "../../api/import";
import { useDryRunImport, useRunImport } from "./queries";
import { ImportErrorTable } from "./ImportErrorTable";
import { CsvPreview } from "./CsvPreview";

const SAMPLE_ROW = "特定口座,7203,buy,100,2500.00,275,2026-04-15,,";

export function ImportPage() {
  const [csv, setCsvRaw] = useState("");
  const [encoding, setEncoding] = useState<CsvEncoding>("utf-8");
  const [report, setReport] = useState<ImportReport | null>(null);
  const [done, setDone] = useState<ImportCounts | null>(null);
  const [inputError, setInputError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const dryRun = useDryRunImport();
  const run = useRunImport();

  /** CSVが変わったら検証結果は無効。ここを通さずに setCsv しないこと */
  function setCsv(next: string) {
    setCsvRaw(next);
    setReport(null);
    setDone(null);
    setInputError(null);
    dryRun.reset();
    run.reset();
  }

  async function handleFile(file: File | undefined) {
    if (!file) return;
    try {
      setCsv(await readCsvFile(file, encoding));
    } catch (err) {
      setCsv("");
      setInputError(err instanceof CsvInputError ? err.message : "ファイルを読み込めませんでした");
    }
  }

  const rowCount = countDataRows(csv);
  const tooManyRows = rowCount > CSV_MAX_ROWS;
  const canValidate = csv.trim() !== "" && !tooManyRows && !dryRun.isPending;
  const canSubmit =
    report !== null && !hasErrors(report) && report.to_insert > 0 && !run.isPending;

  async function handleDryRun() {
    const result = await dryRun.mutateAsync(csv);
    setReport(result);
  }

  async function handleRun() {
    const outcome = await run.mutateAsync(csv);
    if (outcome.kind === "inserted") {
      setDone(countsFromResult(outcome.result));
      setReport(null);
    } else {
      // 検証後にデータが変わったなど。差し替わった検証結果を表示する
      setReport(outcome.report);
    }
  }

  function downloadTemplate() {
    const blob = new Blob([`${CSV_TEMPLATE_HEADER}\n${SAMPLE_ROW}\n`], {
      type: "text/csv;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "transactions-template.csv";
    anchor.click();
    URL.revokeObjectURL(url);
  }

  const failure = [dryRun.error, run.error].find(Boolean);

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <header className="space-y-1">
        <h1 className="text-xl font-bold">CSVインポート</h1>
        <p className="text-sm text-gray-600">
          取引を一括登録します。まず検証してから本登録します。口座・銘柄は事前に登録が必要です。
        </p>
      </header>

      <section className="space-y-3 rounded border border-gray-300 p-4">
        <div className="flex flex-wrap items-center gap-3">
          <input
            ref={fileRef}
            type="file"
            accept=".csv,text/csv"
            onChange={(e) => void handleFile(e.target.files?.[0])}
            className="text-sm"
          />
          <label className="flex items-center gap-2 text-sm">
            文字コード
            <select
              value={encoding}
              onChange={(e) => setEncoding(e.target.value as CsvEncoding)}
              className="rounded border border-gray-300 px-2 py-1"
            >
              <option value="utf-8">UTF-8</option>
              <option value="shift_jis">Shift_JIS</option>
            </select>
          </label>
          <button
            type="button"
            onClick={downloadTemplate}
            className="text-sm text-blue-700 underline"
          >
            テンプレートをダウンロード
          </button>
        </div>

        <textarea
          value={csv}
          onChange={(e) => setCsv(e.target.value)}
          rows={8}
          spellCheck={false}
          placeholder={`${CSV_TEMPLATE_HEADER}\n${SAMPLE_ROW}`}
          className="w-full rounded border border-gray-300 p-2 font-mono text-xs"
        />

        <div className="flex items-center justify-between text-sm">
          <span className="text-gray-600">
            {rowCount}行（ヘッダ除く）
            {tooManyRows && (
              <span className="ml-2 text-red-700">上限{CSV_MAX_ROWS}行を超えています</span>
            )}
          </span>
          <button
            type="button"
            onClick={() => void handleDryRun()}
            disabled={!canValidate}
            className="rounded bg-gray-800 px-4 py-2 text-white disabled:bg-gray-300"
          >
            {dryRun.isPending ? "検証中..." : "検証する"}
          </button>
        </div>

        {inputError && <p className="text-sm text-red-700">{inputError}</p>}
        {failure instanceof ApiError && (
          <p className="text-sm text-red-700">
            {failure.problem.title}
            {failure.problem.detail ? `: ${failure.problem.detail}` : ""}
          </p>
        )}
      </section>

      {report && (
        <section className="space-y-3">
          <dl className="grid grid-cols-3 gap-3 text-sm">
            <Stat label="読み込み行数" value={report.total_rows} />
            <Stat label="登録される行" value={report.to_insert} />
            <Stat label="重複でスキップ" value={report.to_skip_duplicate} />
          </dl>

          {hasErrors(report) ? (
            <ImportErrorTable errors={report.errors} />
          ) : (
            <div className="flex items-center justify-between rounded border border-green-300 bg-green-50 px-4 py-3">
              <p className="text-sm text-green-900">
                {report.to_insert > 0
                  ? `検証に成功しました。${report.to_insert}行を登録できます。`
                  : "登録対象の行がありません（すべて重複です）。"}
              </p>
              <button
                type="button"
                onClick={() => void handleRun()}
                disabled={!canSubmit}
                className="rounded bg-green-700 px-4 py-2 text-sm text-white disabled:bg-gray-300"
              >
                {run.isPending ? "登録中..." : "本登録する"}
              </button>
            </div>
          )}

          <CsvPreview csv={csv} errors={report.errors} />
        </section>
      )}

      {done && (
        <section className="rounded border border-green-300 bg-green-50 px-4 py-3 text-sm text-green-900">
          <p className="font-semibold">取り込みが完了しました。</p>
          <p>
            {done.inserted}件を登録、{done.skippedDuplicate}件を重複としてスキップしました。
          </p>
          <a href="/transactions" className="mt-1 inline-block underline">
            取引一覧を見る
          </a>
        </section>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded border border-gray-300 p-3">
      <dt className="text-xs text-gray-600">{label}</dt>
      <dd className="text-lg font-semibold tabular-nums">{value}</dd>
    </div>
  );
}