import type {
  AllocationItem,
  AllocationResult,
  HistoryResult,
} from "@/api/analytics";

/* ---------------- 期間プリセット ---------------- */

export type PeriodPreset = "1m" | "3m" | "6m" | "1y" | "all";

export const PERIOD_PRESETS: { value: PeriodPreset; label: string }[] = [
  { value: "1m", label: "1ヶ月" },
  { value: "3m", label: "3ヶ月" },
  { value: "6m", label: "6ヶ月" },
  { value: "1y", label: "1年" },
  { value: "all", label: "全期間" },
];

/** ローカル時刻で YYYY-MM-DD（toISOString はUTCへずれるので使わない） */
export function toDateString(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

const ALL_PERIOD_START = "2000-01-01";

export function resolveRange(
  preset: PeriodPreset,
  today: Date = new Date(),
): { from: string; to: string } {
  const to = toDateString(today);
  if (preset === "all") return { from: ALL_PERIOD_START, to };

  const from = new Date(today);
  const months = preset === "1m" ? 1 : preset === "3m" ? 3 : preset === "6m" ? 6 : 12;
  const targetDay = from.getDate();
  from.setMonth(from.getMonth() - months);
  // 3/31 の1ヶ月前が 3/2 に繰り上がるのを月末へ補正
  if (from.getDate() !== targetDay) from.setDate(0);
  return { from: toDateString(from), to };
}

/** 1年以上は month に落として点数を抑える */
export function resolveGranularity(preset: PeriodPreset): "day" | "month" {
  return preset === "1y" || preset === "all" ? "month" : "day";
}

/* ---------------- 数値・書式 ---------------- */

/** Decimalは文字列で来るので、API境界でここだけ通す */
export function toNumber(v: string | number | null | undefined): number | null {
  if (v === null || v === undefined) return null;
  const n = typeof v === "number" ? v : Number(v);
  return Number.isFinite(n) ? n : null;
}

export function formatYen(v: string | number | null | undefined): string {
  const n = toNumber(v);
  if (n === null) return "—";
  return `¥${Math.round(n).toLocaleString("ja-JP")}`;
}

/**
 * allocation の ratio は既にパーセント（合計100.00）なので100倍しない。
 * #21 の formatRatioAsPercent（比率0〜1を100倍する）とは別物なので名前で分ける。
 */
export function formatPercent(v: string | number | null | undefined): string {
  const n = toNumber(v);
  if (n === null) return "—";
  return `${n.toFixed(2)}%`;
}

export function formatAxisDate(date: string, granularity: "day" | "month"): string {
  const [y, m, d] = date.split("-");
  return granularity === "month" ? `${y}/${m}` : `${m}/${d}`;
}

/* ---------------- 推移: ピボット ---------------- */

/** 系列キーが date と衝突しないよう接頭辞で隔離する */
export const seriesDataKey = (key: string): string => `v:${key}`;

export const COST_DATA_KEY = "v:__cost__";

export type HistoryRow = {
  date: string;
  unpricedCount: number;
  [dataKey: string]: string | number | null;
};

export type HistoryChartData = {
  rows: HistoryRow[];
  seriesMeta: { dataKey: string; label: string }[];
  /** group_by=none 相当（系列1本）なら簿価ラインを引ける */
  isSingleSeries: boolean;
  hasUnpriced: boolean;
};

export function buildHistoryChartData(result: HistoryResult): HistoryChartData {
  const dates = new Set<string>();
  for (const s of result.series) {
    for (const p of s.points) dates.add(p.date);
  }
  const sortedDates = [...dates].sort();

  const rows: HistoryRow[] = sortedDates.map((date) => ({
    date,
    unpricedCount: 0,
  }));
  const rowByDate = new Map(rows.map((r) => [r.date, r]));

  const isSingleSeries = result.series.length === 1;

  for (const s of result.series) {
    for (const p of s.points) {
      const row = rowByDate.get(p.date);
      if (!row) continue;
      row[seriesDataKey(s.key)] = toNumber(p.market_value_jpy);
      // グループは互いに素なので、日ごとの未評価件数は合算でよい
      row.unpricedCount += p.unpriced_asset_count;
      if (isSingleSeries) row[COST_DATA_KEY] = toNumber(p.cost_jpy);
    }
  }

  return {
    rows,
    seriesMeta: result.series.map((s) => ({
      dataKey: seriesDataKey(s.key),
      label: s.label,
    })),
    isSingleSeries,
    hasUnpriced: rows.some((r) => r.unpricedCount > 0),
  };
}

/* ---------------- 配分 ---------------- */

export type AllocationRow = {
  key: string;
  label: string;
  value: number;
  ratio: string;
};

/** ドーナツの見やすさを優先し評価額の降順に並べ替える（サーバー順は保持しない） */
export function buildAllocationRows(result: AllocationResult): AllocationRow[] {
  return result.items
    .map((it: AllocationItem) => ({
      key: it.key,
      label: it.label,
      value: toNumber(it.value_jpy) ?? 0,
      ratio: it.ratio,
    }))
    .sort((a, b) => b.value - a.value);
}

/* ---------------- 配色 ---------------- */

export const CHART_COLORS = [
  "#2563eb",
  "#16a34a",
  "#f59e0b",
  "#dc2626",
  "#7c3aed",
  "#0891b2",
  "#db2777",
  "#65a30d",
] as const;

export const colorAt = (i: number): string =>
  CHART_COLORS[i % CHART_COLORS.length];
