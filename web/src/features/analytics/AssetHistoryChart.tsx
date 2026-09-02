import { useMemo } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { TooltipContentProps } from "recharts";
import type { HistoryResult } from "@/api/analytics";
import {
  COST_DATA_KEY,
  PERIOD_PRESETS,
  buildHistoryChartData,
  colorAt,
  formatAxisDate,
  formatYen,
} from "./format";
import type { PeriodPreset } from "./format";

export type HistoryGroupBy = "none" | "account_type" | "asset_class";

const GROUP_BY_OPTIONS: { value: HistoryGroupBy; label: string }[] = [
  { value: "none", label: "合計" },
  { value: "account_type", label: "口座種別" },
  { value: "asset_class", label: "資産クラス" },
];

type Props = {
  result: HistoryResult | null;
  loading: boolean;
  error: string | null;
  preset: PeriodPreset;
  onPresetChange: (p: PeriodPreset) => void;
  groupBy: HistoryGroupBy;
  onGroupByChange: (g: HistoryGroupBy) => void;
  onRetry: () => void;
};

/** 軸ラベルは万・億に丸めて桁あふれを防ぐ */
function formatAxisYen(v: number): string {
  const abs = Math.abs(v);
  if (abs >= 1e8) return `${(v / 1e8).toFixed(1)}億`;
  if (abs >= 1e4) return `${Math.round(v / 1e4).toLocaleString("ja-JP")}万`;
  return String(Math.round(v));
}

function ChartTooltip({
  active,
  payload,
  label,
}: TooltipContentProps) {
  if (!active || !payload?.length) return null;
  const unpriced = (payload[0]?.payload as { unpricedCount?: number })
    ?.unpricedCount;

  return (
    <div className="rounded-md border border-slate-200 bg-white px-3 py-2 text-sm shadow-sm">
      <div className="mb-1 font-medium text-slate-900">{label}</div>
      {payload.map((p) => (
        <div key={p.dataKey as string} className="flex items-center gap-2 py-0.5">
          <span
            className="h-2 w-2 shrink-0 rounded-full"
            style={{ background: p.color }}
          />
          <span className="text-slate-600">{p.name}</span>
          <span className="ml-auto font-medium tabular-nums text-slate-900">
            {formatYen(typeof p.value === "object" ? null : p.value)}
          </span>
        </div>
      ))}
      {unpriced ? (
        <div className="mt-1 border-t border-slate-100 pt-1 text-xs text-amber-700">
          未評価 {unpriced} 銘柄
        </div>
      ) : null}
    </div>
  );
}

export function AssetHistoryChart({
  result,
  loading,
  error,
  preset,
  onPresetChange,
  groupBy,
  onGroupByChange,
  onRetry,
}: Props) {
  const data = useMemo(
    () => (result ? buildHistoryChartData(result) : null),
    [result],
  );

  const granularity = result?.granularity ?? "day";
  const stacked = groupBy !== "none";

  const segmentClass = (active: boolean) =>
    `px-3 py-1.5 text-sm border-r border-slate-300 last:border-r-0 ${
      active ? "bg-slate-900 text-white" : "text-slate-600 hover:bg-slate-50"
    }`;

  const controls = (
    <div className="flex flex-wrap items-center gap-3">
      <div
        className="inline-flex overflow-hidden rounded-md border border-slate-300"
        role="group"
        aria-label="期間"
      >
        {PERIOD_PRESETS.map((p) => (
          <button
            key={p.value}
            type="button"
            className={segmentClass(p.value === preset)}
            aria-pressed={p.value === preset}
            onClick={() => onPresetChange(p.value)}
          >
            {p.label}
          </button>
        ))}
      </div>
      <div
        className="inline-flex overflow-hidden rounded-md border border-slate-300"
        role="group"
        aria-label="分類"
      >
        {GROUP_BY_OPTIONS.map((g) => (
          <button
            key={g.value}
            type="button"
            className={segmentClass(g.value === groupBy)}
            aria-pressed={g.value === groupBy}
            onClick={() => onGroupByChange(g.value)}
          >
            {g.label}
          </button>
        ))}
      </div>
    </div>
  );

  let body: React.ReactNode;

  if (loading && !data) {
    body = <p className="py-12 text-center text-sm text-slate-500">読み込み中…</p>;
  } else if (error) {
    body = (
      <div
        role="alert"
        className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700"
      >
        <p>{error}</p>
        <button
          type="button"
          onClick={onRetry}
          className="mt-2 rounded-md border border-red-300 px-3 py-1 text-red-700"
        >
          再読み込み
        </button>
      </div>
    );
  } else if (!data || data.rows.length === 0) {
    body = (
      <p className="py-12 text-center text-sm text-slate-500">
        この期間に表示できるデータがありません。取引と価格を登録すると推移が表示されます。
      </p>
    );
  } else {
    body = (
      <ResponsiveContainer width="100%" height={320}>
        {stacked ? (
          <AreaChart data={data.rows} margin={{ top: 8, right: 8, bottom: 0, left: 8 }}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} />
            <XAxis
              dataKey="date"
              tickFormatter={(d: string) => formatAxisDate(d, granularity)}
              minTickGap={24}
            />
            <YAxis tickFormatter={formatAxisYen} width={64} />
            <Tooltip content={(props) => <ChartTooltip {...props} />} />
            <Legend />
            {data.seriesMeta.map((s, i) => (
              <Area
                key={s.dataKey}
                type="monotone"
                dataKey={s.dataKey}
                name={s.label}
                stackId="value"
                stroke={colorAt(i)}
                fill={colorAt(i)}
                fillOpacity={0.35}
                connectNulls
              />
            ))}
          </AreaChart>
        ) : (
          <LineChart data={data.rows} margin={{ top: 8, right: 8, bottom: 0, left: 8 }}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} />
            <XAxis
              dataKey="date"
              tickFormatter={(d: string) => formatAxisDate(d, granularity)}
              minTickGap={24}
            />
            <YAxis tickFormatter={formatAxisYen} width={64} />
            <Tooltip content={(props) => <ChartTooltip {...props} />} />
            <Legend />
            {data.seriesMeta.map((s, i) => (
              <Line
                key={s.dataKey}
                type="monotone"
                dataKey={s.dataKey}
                name={s.label}
                stroke={colorAt(i)}
                dot={false}
                connectNulls
              />
            ))}
            {data.isSingleSeries && (
              <Line
                type="monotone"
                dataKey={COST_DATA_KEY}
                name="簿価"
                stroke="#94a3b8"
                strokeDasharray="4 4"
                dot={false}
                connectNulls
              />
            )}
          </LineChart>
        )}
      </ResponsiveContainer>
    );
  }

  return (
    <section className="rounded-lg border border-slate-200 bg-white p-4">
      <header className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-lg font-semibold text-slate-900">資産推移</h2>
        {controls}
      </header>

      {result?.fx_stale && (
        <p className="mb-3 rounded bg-amber-50 px-3 py-2 text-sm text-amber-800">
          為替レートを取得できず、キャッシュした値で換算しています。
        </p>
      )}
      {data?.hasUnpriced && (
        <p className="mb-3 rounded bg-amber-50 px-3 py-2 text-sm text-amber-800">
          価格を引けなかった銘柄が一部の日で評価から除外されています。
        </p>
      )}

      {body}
    </section>
  );
}