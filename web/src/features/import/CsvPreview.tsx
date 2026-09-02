import { indexErrorsByRow, parsePreview } from "./preview";
import type { ImportRowError } from "../../api/import";

type Props = {
  csv: string;
  errors: readonly ImportRowError[];
};

export function CsvPreview({ csv, errors }: Props) {
  const { header, rows } = parsePreview(csv);
  const errorRows = indexErrorsByRow(errors);
  if (rows.length === 0) return null;

  return (
    <div className="max-h-96 overflow-auto rounded border border-gray-300">
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-gray-100">
          <tr className="text-left">
            <th className="w-14 px-3 py-2 font-medium text-gray-600">行</th>
            {header.map((name) => (
              <th key={name} className="px-3 py-2 font-medium text-gray-600">{name}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const bad = errorRows.has(row.row);
            return (
              <tr
                key={row.row}
                className={bad ? "border-t border-red-200 bg-red-50" : "border-t border-gray-200"}
              >
                <td className="px-3 py-1.5 text-right tabular-nums text-gray-500">{row.row}</td>
                {row.cells.map((cell, index) => (
                  <td key={index} className="px-3 py-1.5 whitespace-nowrap">{cell}</td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}