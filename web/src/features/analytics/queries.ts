import { keepPreviousData, useQuery } from "@tanstack/react-query";
import {
  getAllocation,
  getAssetHistory,
  type AllocationQuery,
  type AssetHistoryQuery,
} from "@/api/analytics";
import { ApiError } from "@/api/problem";
import { queryKeys } from "@/lib/queryKeys";

// 422（未来日、from が to より後、期間が多すぎる等）は再試行しても結果が変わらない
const isClientError = (error: unknown) =>
  error instanceof ApiError && error.status >= 400 && error.status < 500;

export function useAssetHistory(query: AssetHistoryQuery) {
  return useQuery({
    queryKey: [...queryKeys.analytics, "asset-history", query] as const,
    queryFn: () => getAssetHistory(query),
    // 期間・分類を切り替えるたびグラフ全体が読み込み中に戻るのを防ぐ
    placeholderData: keepPreviousData,
    retry: (failureCount, error) =>
      isClientError(error) ? false : failureCount < 3,
  });
}

export function useAllocation(query: AllocationQuery) {
  return useQuery({
    queryKey: [...queryKeys.analytics, "allocation", query] as const,
    queryFn: () => getAllocation(query),
    placeholderData: keepPreviousData,
    retry: (failureCount, error) =>
      isClientError(error) ? false : failureCount < 3,
  });
}
