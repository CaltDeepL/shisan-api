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

export const assetsKey = ["assets"] as const;

export function useAssets(q: string) {
  return useQuery({
    queryKey: [...assetsKey, q] as const,
    queryFn: () => listAssets(q),
    // 検索語ごとにキャッシュが分かれるので、切り替え中に一覧が空にならないようにする
    placeholderData: keepPreviousData,
  });
}

export function useCreateAsset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAssetRequest) => createAsset(body),
    onSuccess: () => qc.invalidateQueries({ queryKey: assetsKey }),
  });
}

export function useUpdateAsset(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: PatchAssetRequest) => patchAsset(id, body),
    onSuccess: () => qc.invalidateQueries({ queryKey: assetsKey }),
  });
}

export const pricesKey = (assetId: string) =>
  [...assetsKey, assetId, "prices"] as const;

export function usePrices(assetId: string) {
  return useQuery({
    queryKey: pricesKey(assetId),
    queryFn: () => listPrices(assetId),
  });
}

export function useUpsertPrices(assetId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpsertPricesRequest) => upsertPrices(body),
    onSuccess: () => qc.invalidateQueries({ queryKey: pricesKey(assetId) }),
  });
}
