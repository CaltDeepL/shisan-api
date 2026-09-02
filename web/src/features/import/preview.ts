export type PreviewRow = {
  /** ImportRowError.row と同じ体系（ヘッダを除く1始まり） */
  row: number;
  cells: string[];
};

export type Preview = {
  header: string[];
  rows: PreviewRow[];
};

/**
 * 表示専用の簡易パース。引用符で囲まれたカンマは考慮しない。
 * サーバー側の判定は csv クレートが行うため、ここでのズレは表示にしか影響しない。
 */
export function parsePreview(csv: string): Preview {
  const lines = csv.split(/\r?\n/).filter((line) => line.trim() !== "");
  if (lines.length === 0) return { header: [], rows: [] };
  const [headerLine, ...dataLines] = lines;
  return {
    header: headerLine.split(",").map((cell) => cell.trim()),
    rows: dataLines.map((line, index) => ({
      row: index + 1, // ヘッダを除く1始まり = ImportRowError.row
      cells: line.split(","),
    })),
  };
}

/** row → メッセージ配列。同じ行に複数エラーが付く可能性を考慮して配列で持つ */
export function indexErrorsByRow(
  errors: readonly { row: number; message: string }[],
): Map<number, string[]> {
  const map = new Map<number, string[]>();
  for (const error of errors) {
    const list = map.get(error.row);
    if (list) list.push(error.message);
    else map.set(error.row, [error.message]);
  }
  return map;
}