const jpy = new Intl.NumberFormat("ja-JP", {
  maximumFractionDigits: 0,
});

const qty = new Intl.NumberFormat("ja-JP", {
  minimumFractionDigits: 0,
  maximumFractionDigits: 8,
});

const unitPrice = new Intl.NumberFormat("ja-JP", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/** null は em dash。空欄にするとデータ欠損か表示バグか区別がつかない */
export const DASH = "—";

/** 金額。小数は落とす */
export function formatMoney(value: string | null | undefined): string {
  if (value === null) return DASH;
  return jpy.format(Number(value));
}

/** 数量。numeric(20,8) の末尾ゼロを落としつつ3桁区切りを入れる */
export function formatQuantity(value: string): string {
  return qty.format(Number(value));
}

/** 平均取得単価・現在値。端数が意味を持つので2桁まで出す */
export function formatUnitPrice(value: string | null | undefined): string {
  if (value === null) return DASH;
  return unitPrice.format(Number(value));
}

/**
 * 比率（0.05 = +5%）をパーセント表示にする。
 * ⚠ holdings の unrealized_pnl_rate は比率、allocation の ratio は
 * 最初からパーセント値（33.34 など）。単位が混在しているため関数を分けている。
 * 100倍はこちらだけ。
 */
export function formatRatioAsPercent(value: string | null | undefined): string {
  if (value === null) return DASH;
  const n = Number(value) * 100;
  return `${n >= 0 ? "+" : ""}${n.toFixed(2)}%`;
}

/** 損益額。率と同様に符号を明示する */
export function formatSignedMoney(value: string | null | undefined): string {
  if (value === null) return DASH;
  const n = Number(value);
  return `${n > 0 ? "+" : ""}${jpy.format(n)}`;
}

/** 損益の符号に対応する Tailwind クラス。色は符号の補助であって主表現ではない */
export function pnlClass(value: string | null | undefined): string {
  if (value === null) return "text-slate-400";
  const n = Number(value);
  if (n > 0) return "text-emerald-600";
  if (n < 0) return "text-rose-600";
  return "text-slate-600";
}