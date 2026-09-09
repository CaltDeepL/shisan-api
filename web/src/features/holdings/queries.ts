import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { listHoldings, type HoldingsQuery } from "@/api/holdings";
import { ApiError } from "@/api/problem";
import { queryKeys } from "@/lib/queryKeys";

export function useHoldings(query: HoldingsQuery = {}) {
  return useQuery({
    queryKey: [...queryKeys.holdings, query] as const,
    queryFn: () => listHoldings(query),
    // 口座やトグルを切り替えるたび表全体がスケルトンに戻るのを防ぐ
    placeholderData: keepPreviousData,
    // 404（存在しない口座）は再試行しても結果が変わらない
    retry: (failureCount, error) =>
      error instanceof ApiError && error.status === 404 ? false : failureCount < 3,
  });
}
