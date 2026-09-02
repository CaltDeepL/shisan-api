import type { ImportRowError } from "../../api/import";

type Props = { errors: readonly ImportRowError[] };

export function ImportErrorTable({ errors }: Props) {
  return (
    <div className="rounded border border-red-300 bg-red-50">
      <p className="border-b border-red-200 px-4 py-2 text-sm font-semibold text-red-800">
        {errors.length}件のエラーがあります。1件でも残っていると全行が取り込まれません。
      </p>
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-red-900">
            <th className="w-20 px-4 py-2 font-medium">行</th>
            <th className="px-4 py-2 font-medium">内容</th>
          </tr>
        </thead>
        <tbody>
          {errors.map((error, index) => (
            <tr key={`${error.row}-${index}`} className="border-t border-red-200">
              <td className="px-4 py-2 text-right tabular-nums text-red-900">{error.row}</td>
              <td className="px-4 py-2 text-red-900">{error.message}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="border-t border-red-200 px-4 py-2 text-xs text-red-700">
        口座・銘柄は事前に登録が必要です。未登録なら
        <a href="/accounts" className="underline">口座</a> /
        <a href="/assets" className="underline">銘柄</a> から登録してください。
      </p>
    </div>
  );
}