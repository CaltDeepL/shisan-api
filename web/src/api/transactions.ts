import { apiFetch } from "./client";
import type { components } from "./schema";

export type Transaction = components["schemas"]["TransactionResponse"];
export type TradeKind = components["schemas"]["TradeKind"];
export type CreateTransaction = components["schemas"]["CreateTransaction"];

export type TransactionFilter = {
  account_id?: string;
  asset_id?: string;
  from?: string;
  to?: string;
};

export function listTransactions(
  filter: TransactionFilter,
): Promise<Transaction[]> {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(filter)) {
    if (v) params.set(k, v);
  }
  const query = params.toString();
  return apiFetch<Transaction[]>(`/transactions${query ? `?${query}` : ""}`);
}

export function createTransaction(
  body: CreateTransaction,
): Promise<Transaction> {
  return apiFetch<Transaction>("/transactions", { method: "POST", body });
}

export function deleteTransaction(id: string): Promise<void> {
  return apiFetch<void>(`/transactions/${id}`, { method: "DELETE" });
}

/** 入力フォームの値。数値は Decimal を崩さないよう文字列で保持する。 */
export type TransactionFormValues = {
  account_id: string;
  asset_id: string;
  kind: TradeKind;
  quantity: string;
  price: string;
  fee: string;
  traded_at: string;
  note: string;
};

export function buildCreateTransaction(
  values: TransactionFormValues,
): CreateTransaction {
  const body: CreateTransaction = {
    account_id: values.account_id,
    asset_id: values.asset_id,
    kind: values.kind,
    quantity: values.quantity.trim(),
    price: values.price.trim(),
    traded_at: values.traded_at,
  };
  // fee は未指定なら0がサーバー既定
  const fee = values.fee.trim();
  if (fee !== "") body.fee = fee;
  // note は空白のみだと 422。空欄はキーごと省く
  const note = values.note.trim();
  if (note !== "") body.note = note;
  return body;
}