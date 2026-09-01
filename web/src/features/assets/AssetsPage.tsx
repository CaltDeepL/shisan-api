import { useEffect, useState } from "react";
import type { Asset } from "@/api/assets";
import { useAssets } from "./queries";
import { assetClassLabels } from "./labels";
import { CreateAssetDialog } from "./CreateAssetDialog";
import { EditAssetDialog } from "./EditAssetDialog";
import { PriceDialog } from "./PriceDialog";

/** 入力の落ち着きを待ってから検索する（1文字ごとにAPIを叩かない） */
function useDebounced(value: string, ms: number) {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

export function AssetsPage() {
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Asset | null>(null);
  const [q, setQ] = useState("");
  const debouncedQ = useDebounced(q, 300);
  const { data, isPending, isError, error, refetch } = useAssets(debouncedQ);
  const [pricing, setPricing] = useState<Asset | null>(null);

  const addButton = (
    <button
      type="button"
      onClick={() => setCreating(true)}
      className="rounded-md bg-slate-900 px-4 py-2 text-sm text-white hover:bg-slate-700"
    >
      銘柄を追加
    </button>
  );

  if (isPending) {
    return <p className="text-sm text-slate-500">読み込み中…</p>;
  }

  if (isError) {
    return (
      <div
        role="alert"
        className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700"
      >
        <p>銘柄の取得に失敗しました。</p>
        <p className="mt-1 text-red-500">{error.message}</p>
        <button
          type="button"
          onClick={() => void refetch()}
          className="mt-2 rounded-md border border-red-300 px-3 py-1 text-red-700"
        >
          再試行
        </button>
      </div>
    );
  }

  const searching = debouncedQ.trim() !== "";

  return (
    <section className="space-y-6">
      <header className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-slate-900">銘柄</h1>
        {(data.length > 0 || searching) && addButton}
      </header>

      {(data.length > 0 || searching) && (
        <div className="space-y-1">
          <label htmlFor="asset-search" className="sr-only">
            銘柄を検索
          </label>
          <input
            id="asset-search"
            type="search"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="コードまたは名称で検索"
            className="w-full max-w-sm rounded-md border border-slate-300 px-3 py-2 text-slate-900 outline-none focus:ring-2 focus:ring-slate-300"
          />
        </div>
      )}

      {data.length === 0 ? (
        searching ? (
          <p className="text-sm text-slate-500">
            「{debouncedQ}」に一致する銘柄はありません。
          </p>
        ) : (
          <div className="rounded-lg border border-dashed border-slate-300 px-6 py-12 text-center">
            <p className="text-slate-900">まだ銘柄が登録されていません。</p>
            <p className="mt-1 text-sm text-slate-500">
              保有している株式や投資信託を登録すると、取引を記録できます。
            </p>
            <div className="mt-4">{addButton}</div>
          </div>
        )
      ) : (
        <div className="overflow-hidden rounded-lg border border-slate-200">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 text-left text-slate-600">
              <tr>
                <th scope="col" className="px-4 py-2 font-medium">
                  コード
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  名称
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  資産クラス
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  通貨
                </th>
                <th scope="col" className="px-4 py-2 text-right font-medium">
                  価格単位
                </th>
                <th scope="col" className="px-4 py-2">
                  <span className="sr-only">操作</span>
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200 text-slate-900">
              {data.map((a) => (
                <tr key={a.id}>
                  <td className="px-4 py-2 font-mono">{a.symbol}</td>
                  <td className="px-4 py-2">{a.name}</td>
                  <td className="px-4 py-2">
                    {assetClassLabels[a.asset_class]}
                  </td>
                  <td className="px-4 py-2">{a.currency}</td>
                  <td className="px-4 py-2 text-right tabular-nums">
                    {a.price_unit}
                  </td>
                  <td className="px-4 py-2 text-right">
                    <button
                      type="button"
                      onClick={() => setEditing(a)}
                      className="rounded-md border border-slate-300 px-3 py-1 text-slate-700"
                    >
                      編集
                    </button>
                    <button
                      type="button"
                      onClick={() => setPricing(a)}
                      className="ml-2 rounded-md border border-slate-300 px-3 py-1 text-slate-700"
                    >
                      価格
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <CreateAssetDialog open={creating} onClose={() => setCreating(false)} />
      {editing && (
        <EditAssetDialog
          key={editing.id}
          asset={editing}
          onClose={() => setEditing(null)}
        />
      )}
      {pricing && (
        <PriceDialog
          key={pricing.id}
          asset={pricing}
          onClose={() => setPricing(null)}
        />
      )}
    </section>
  );
}