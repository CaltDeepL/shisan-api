import { apiFetch } from "./client";
import type { components, operations } from "./schema";

export type AllocationQuery = NonNullable<
  operations["get_allocation"]["parameters"]["query"]
>;
export type AllocationResult = components["schemas"]["AllocationResult"];
export type AllocationItem = components["schemas"]["AllocationItem"];

export type AssetHistoryQuery = NonNullable<
  operations["get_asset_history"]["parameters"]["query"]
>;
export type HistoryResult = components["schemas"]["HistoryResult"];
export type HistorySeries = components["schemas"]["HistorySeries"];
export type HistoryPoint = components["schemas"]["HistoryPoint"];

/** undefined を落としてクエリ文字列にする(?だけ付く事故を防ぐ) */
function toQuery(params: Record<string, string | number | undefined>): string {
  const sp = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== "") sp.set(k, String(v));
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

export function getAllocation(
  query: AllocationQuery = {},
): Promise<AllocationResult> {
  return apiFetch<AllocationResult>(
    `/analytics/allocation${toQuery(query as Record<string, string | undefined>)}`,
  );
}

export function getAssetHistory(
  query: AssetHistoryQuery = {},
): Promise<HistoryResult> {
  return apiFetch<HistoryResult>(
    `/analytics/asset-history${toQuery(query as Record<string, string | undefined>)}`,
  );
}
