/** null/undefined は em dash。空欄にするとデータ欠損か表示バグか区別がつかない */
export const DASH = "—";

/** Decimalは文字列で来るので、API境界でここだけ通す */
export function toNumber(value: string | number | null | undefined): number | null {
  if (value === null || value === undefined) return null;
  const n = typeof value === "number" ? value : Number(value);
  return Number.isFinite(n) ? n : null;
}

const jpy = new Intl.NumberFormat("ja-JP", { maximumFractionDigits: 0 });

/** 金額。通貨記号なし（保有一覧など複数通貨が混在しうる画面向け）。小数は落とす */
export function formatMoney(value: string | number | null | undefined): string {
  const n = toNumber(value);
  return n === null ? DASH : jpy.format(n);
}

/** 符号付き金額。通貨記号なし */
export function formatSignedMoney(value: string | number | null | undefined): string {
  const n = toNumber(value);
  if (n === null) return DASH;
  return `${n > 0 ? "+" : ""}${jpy.format(n)}`;
}

/** JPY固定の金額。¥記号付き（分析画面など、常にJPY換算されている画面向け） */
export function formatYen(value: string | number | null | undefined): string {
  const n = toNumber(value);
  return n === null ? DASH : `¥${jpy.format(n)}`;
}

/** 損益の符号に対応する Tailwind クラス。色は符号の補助であって主表現ではない */
export function pnlClass(value: string | number | null | undefined): string {
  const n = toNumber(value);
  if (n === null) return "text-slate-400";
  if (n > 0) return "text-emerald-600";
  if (n < 0) return "text-rose-600";
  return "text-slate-600";
}

/**
 * 比率（0.05 = +5%）をパーセント表示にする。符号付き。100倍はこちらだけ。
 * すでにパーセント値（33.34 など）を表示したい場合は `formatPercent` を使う。
 */
export function formatRatioAsPercent(value: string | number | null | undefined): string {
  const n = toNumber(value);
  if (n === null) return DASH;
  const pct = n * 100;
  return `${pct >= 0 ? "+" : ""}${pct.toFixed(2)}%`;
}

/** すでにパーセント値（33.34 など）をそのまま表示する。符号なし */
export function formatPercent(value: string | number | null | undefined): string {
  const n = toNumber(value);
  return n === null ? DASH : `${n.toFixed(2)}%`;
}
