import type { AssetClass } from "@/api/assets";

export const assetClassOptions = [
  "equity",
  "etf",
  "mutual_fund",
  "bond",
  "cash",
  "other",
] as const satisfies readonly AssetClass[];

export const assetClassLabels: Record<AssetClass, string> = {
  equity: "株式",
  etf: "ETF",
  mutual_fund: "投資信託",
  bond: "債券",
  cash: "現金",
  other: "その他",
};

/** 通貨は自由入力をやめてこの2択にする（#19 引き継ぎ 6.3） */
export const currencyOptions = ["JPY", "USD"] as const;
