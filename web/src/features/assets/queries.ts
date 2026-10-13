import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createAsset,
  listAssets,
  listPrices,
  patchAsset,
  upsertPrices,
  type CreateAssetRequest,
  type PatchAssetRequest,
  type UpsertPricesRequest,
} from "@/api/assets";
import { invalidateQueryRoots, queryKeys } from "@/lib/queryKeys";

export function useAssets(q: string) {
  return useQuery({
    queryKey: [...queryKeys.assets, q] as const,
    queryFn: () => listAssets(q),
    // 検索語ごとにキャッシュが分かれるので、切り替え中に一覧が空にならないようにする
    placeholderData: keepPreviousData,
  });
}

export function useCreateAsset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAssetRequest) => createAsset(body),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.assets,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}

export function useUpdateAsset(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: PatchAssetRequest) => patchAsset(id, body),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.assets,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}

export function usePrices(assetId: string) {
  return useQuery({
    queryKey: queryKeys.prices(assetId),
    queryFn: () => listPrices(assetId),
  });
}

export function useUpsertPrices(assetId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpsertPricesRequest) => upsertPrices(body),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.prices(assetId),
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}
