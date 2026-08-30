import type { AccountType } from "@/api/accounts";

/**
 * Record<AccountType, string> にしているのは、ENUM に値を足したときに
 * ここでコンパイルエラーを出すため（domain/account.rs の is_tax_exempt と同じ意図）。
 */
export const accountTypeLabels: Record<AccountType, string> = {
  tokutei: "特定口座",
  ippan: "一般口座",
  nisa_tsumitate: "NISA（つみたて投資枠）",
  nisa_growth: "NISA（成長投資枠）",
  ideco: "iDeCo",
  bank: "銀行口座",
};

/** select の並び順。Record のキー順に依存しないよう明示する。 */
export const accountTypeOptions: AccountType[] = [
  "tokutei",
  "ippan",
  "nisa_tsumitate",
  "nisa_growth",
  "ideco",
  "bank",
];

export function withholdingLabel(withholding: boolean | null): string {
  if (withholding === null) return "—";
  return withholding ? "源泉徴収あり" : "源泉徴収なし";
}
