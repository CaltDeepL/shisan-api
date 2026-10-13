import type { QueryClient, QueryKey } from "@tanstack/react-query";

/**
 * 画面をまたいで共有するキャッシュのルートキー。
 * mutation 後の失効対象を文字列リテラルで重複させないため、ここを唯一の定義元にする。
 */
export const queryKeys = {
  accounts: ["accounts"] as const,
  assets: ["assets"] as const,
  transactions: ["transactions"] as const,
  holdings: ["holdings"] as const,
  analytics: ["analytics"] as const,
  prices: (assetId: string) => ["assets", assetId, "prices"] as const,
};

export async function invalidateQueryRoots(
  queryClient: QueryClient,
  ...roots: QueryKey[]
): Promise<void> {
  await Promise.all(
    roots.map((queryKey) => queryClient.invalidateQueries({ queryKey })),
  );
}
