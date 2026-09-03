import { DASH, toNumber } from "@/lib/format";

export {
  DASH,
  formatMoney,
  formatRatioAsPercent,
  formatSignedMoney,
  pnlClass,
} from "@/lib/format";

const qty = new Intl.NumberFormat("ja-JP", {
  minimumFractionDigits: 0,
  maximumFractionDigits: 8,
});

const unitPrice = new Intl.NumberFormat("ja-JP", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/** 数量。numeric(20,8) の末尾ゼロを落としつつ3桁区切りを入れる */
export function formatQuantity(value: string): string {
  return qty.format(Number(value));
}

/** 平均取得単価・現在値。端数が意味を持つので2桁まで出す */
export function formatUnitPrice(value: string | null | undefined): string {
  const n = toNumber(value);
  return n === null ? DASH : unitPrice.format(n);
}
