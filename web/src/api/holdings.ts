import { apiFetch } from "./client";
import type { components, operations } from "./schema";

export type HoldingsResponse = components["schemas"]["HoldingsResponse"];
export type HoldingItem = components["schemas"]["HoldingItem"];
export type Totals = components["schemas"]["Totals"];
export type HoldingsSummary = components["schemas"]["HoldingsSummary"];
export type AccountSummary = components["schemas"]["AccountSummary"];

export type HoldingsQuery = NonNullable<
  operations["list_holdings"]["parameters"]["query"]
>;

export function listHoldings(
  query: HoldingsQuery = {},
): Promise<HoldingsResponse> {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) {
    if (v) params.set(k, String(v));
  }
  const qs = params.toString();
  return apiFetch<HoldingsResponse>(`/holdings${qs ? `?${qs}` : ""}`);
}

/** 価格が登録済みの保有行。5フィールドが揃って非nullであることを型で表す。 */
export type PricedHolding = HoldingItem & {
  price: string;
  priced_on: string;
  market_value: string;
  unrealized_pnl: string;
  unrealized_pnl_rate: string;
};

/**
 * 価格登録済みかを判定する。
 * サーバー契約上「price が非nullなら残り4つも非null」なので判定は price の1点のみ。
 * 前提が崩れたときに直す箇所を、この関数1つに閉じ込めている。
 */
export function isPriced(h: HoldingItem): h is PricedHolding {
  return h.price !== null;
}