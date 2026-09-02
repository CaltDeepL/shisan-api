import { useMemo } from "react";
import { Pie, PieChart, ResponsiveContainer, Sector, Tooltip } from "recharts";
import type { TooltipContentProps } from "recharts";
import type { AllocationResult } from "@/api/analytics";
import {
  buildAllocationRows,
  colorAt,
  formatPercent,
  formatYen,
} from "./format";

export type AllocationGroupBy =
  | "asset_class"
  | "account_type"
  | "account"
  | "asset";

const GROUP_BY_OPTIONS: { value: AllocationGroupBy; label: string }[] = [
  { value: "asset_class", label: "資産クラス" },
  { value: "account_type", label: "口座種別" },
  { value: "account", label: "口座" },
  { value: "asset", label: "銘柄" },
];

type Props = {
  result: AllocationResult | null;
  loading: boolean;
  error: string | null;
  groupBy: AllocationGroupBy;
  onGroupByChange: (g: AllocationGroupBy) => void;
  onRetry: () => void;
};

/** ③の ChartTooltip と同じく、v3 の props 型・ValueType ガードに合わせる */
function AllocationTooltip({ active, payload }: TooltipContentProps) {
  if (!active || !payload?.length) return null;
  const item = payload[0];
  const value = typeof item?.value === "object" ? null : item?.value;
  const ratio = (item?.payload as { ratio?: string } | undefined)?.ratio;

  return (
    <div className="rounded-md border border-slate-200 bg-white px-3 py-2 text-sm shadow-sm">
      <div className="font-medium text-slate-900">{item?.name}</div>
      <div className="text-slate-600">{formatYen(value)}</div>
      <div className="text-slate-600">
        {formatPercent(ratio ?? null)}
      </div>
    </div>
  );
}

export function AllocationChart({
  result,
  loading,
  error,
  groupBy,
  onGroupByChange,
  onRetry,
}: Props) {
  const rows = useMemo(
    () => (result ? buildAllocationRows(result) : []),
    [result],
  );

  const tabs = (
    <div
      className="inline-flex overflow-hidden rounded-md border border-slate-300"
      role="group"
      aria-label="分類"
    >
      {GROUP_BY_OPTIONS.map((g) => (
        <button
          key={g.value}
          type="button"
          aria-pressed={g.value === groupBy}
          onClick={() => onGroupByChange(g.value)}
          className={
            g.value === groupBy
              ? "bg-slate-900 px-3 py-1.5 text-sm text-white"
              : "bg-white px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50"
          }
        >
          {g.label}
        </button>
      ))}
    </div>
  );

  let body: React.ReactNode;

  if (loading && !result) {
    body = <p className="py-12 text-center text-sm text-slate-500">読み込み中...</p>;
  } else if (error) {
    body = (
      <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
        <p>{error}</p>
        <button
          type="button"
          onClick={onRetry}
          className="mt-2 rounded border border-red-300 bg-white px-2 py-1 text-red-700 hover:bg-red-100"
        >
          再読み込み
        </button>
      </div>
    );
  } else if (rows.length === 0) {
    body = (
      <p className="py-12 text-center text-sm text-slate-500">
        表示できる保有がありません。取引と価格を登録すると配分が表示されます。
      </p>
    );
  } else {
    body = (
      <div className="grid gap-4 md:grid-cols-2">
        <ResponsiveContainer width="100%" height={260}>
          <PieChart>
            <Pie
              data={rows}
              dataKey="value"
              nameKey="label"
              innerRadius="55%"
              outerRadius="85%"
              paddingAngle={1}
              isAnimationActive={false}
              shape={(props) => <Sector {...props} fill={colorAt(props.index)} />}
            />
            <Tooltip content={(props) => <AllocationTooltip {...props} />} />
          </PieChart>
        </ResponsiveContainer>

        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-slate-200 text-left text-slate-500">
              <th className="py-1 font-medium">分類</th>
              <th className="py-1 text-right font-medium">評価額</th>
              <th className="py-1 text-right font-medium">構成比</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={r.key} className="border-b border-slate-100">
                <td className="py-1.5">
                  <span className="flex items-center gap-2">
                    <span
                      className="inline-block h-2.5 w-2.5 rounded-full"
                      style={{ background: colorAt(i) }}
                    />
                    <span className="text-slate-900">{r.label}</span>
                  </span>
                </td>
                <td className="py-1.5 text-right tabular-nums text-slate-900">
                  {formatYen(r.value)}
                </td>
                <td className="py-1.5 text-right tabular-nums text-slate-600">
                  {formatPercent(r.ratio)}
                </td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="font-medium">
              <td className="py-1.5">合計</td>
              <td className="py-1.5 text-right tabular-nums">
                {formatYen(result?.total_value_jpy ?? null)}
              </td>
              <td className="py-1.5 text-right tabular-nums">100.00%</td>
            </tr>
          </tfoot>
        </table>
      </div>
    );
  }

  return (
    <section className="rounded-lg border border-slate-200 bg-white p-4">
      <header className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">資産配分</h2>
          {result && (
            <p className="text-xs text-slate-500">
              {result.as_of} 時点 / 有価証券のみ（現金・預金は含みません）
            </p>
          )}
        </div>
        {tabs}
      </header>

      {result?.fx_stale && (
        <p className="mb-2 rounded bg-amber-50 px-3 py-2 text-sm text-amber-800">
          為替レートを取得できず、キャッシュした値で換算しています。
        </p>
      )}
      {result && result.unpriced_asset_count > 0 && (
        <p className="mb-2 rounded bg-amber-50 px-3 py-2 text-sm text-amber-800">
          価格を引けなかった {result.unpriced_asset_count} 銘柄を配分から除外しています。
        </p>
      )}

      {body}
    </section>
  );
}