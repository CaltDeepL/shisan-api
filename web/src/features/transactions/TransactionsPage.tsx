import { useMemo, useState } from "react";
import type { Transaction, TransactionFilter } from "@/api/transactions";
import { useAccounts } from "@/features/accounts/queries";
import { useAssets } from "@/features/assets/queries";
import { useTransactions } from "./queries";
import { tradeKindLabels } from "./labels";
import { CreateTransactionDialog } from "./CreateTransactionDialog";
import { DeleteTransactionDialog } from "./DeleteTransactionDialog";

export function TransactionsPage() {
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Transaction | null>(null);
  const [filter, setFilter] = useState<TransactionFilter>({});

  const accounts = useAccounts();
  const assets = useAssets(""); // 名前の引き当てとセレクトの選択肢に使う
  const txs = useTransactions(filter);

  const accountName = useMemo(
    () => new Map((accounts.data ?? []).map((a) => [a.id, a.name])),
    [accounts.data],
  );
  const assetLabel = useMemo(
    () =>
      new Map(
        (assets.data ?? []).map((a) => [a.id, `${a.symbol} ${a.name}`]),
      ),
    [assets.data],
  );

  // 一覧は3つのクエリが揃ってから描画する（IDのまま表示される瞬間を作らない）
  const isPending = accounts.isPending || assets.isPending || txs.isPending;
  const failed = [accounts, assets, txs].find((q) => q.isError);

  const canCreate =
    (accounts.data?.length ?? 0) > 0 && (assets.data?.length ?? 0) > 0;

  const addButton = (
    <button
      type="button"
      onClick={() => setCreating(true)}
      disabled={!canCreate}
      className="rounded-md bg-slate-900 px-4 py-2 text-sm text-white hover:bg-slate-700 disabled:opacity-50"
    >
      取引を追加
    </button>
  );

  if (isPending) {
    return <p className="text-sm text-slate-500">読み込み中…</p>;
  }

  if (failed) {
    return (
      <div
        role="alert"
        className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700"
      >
        <p>取引の取得に失敗しました。</p>
        <p className="mt-1 text-red-500">{failed.error?.message}</p>
        <button
          type="button"
          onClick={() => void failed.refetch()}
          className="mt-2 rounded-md border border-red-300 px-3 py-1 text-red-700"
        >
          再試行
        </button>
      </div>
    );
  }

  const filtered = Object.values(filter).some(Boolean);
  const accountList = accounts.data ?? [];
  const assetList = assets.data ?? [];
  const transactions = txs.data ?? [];

  return (
    <section className="space-y-6">
      <header className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-slate-900">取引</h1>
        {(transactions.length > 0 || filtered) && addButton}
      </header>

      {!canCreate && (
        <p className="rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
          取引を登録するには、先に口座と銘柄を登録してください。
        </p>
      )}

      {(transactions.length > 0 || filtered) && (
        <div className="flex flex-wrap items-end gap-3">
          <div className="space-y-1">
            <label htmlFor="filter-account" className="block text-sm font-medium text-slate-700">
              口座
            </label>
            <select
              id="filter-account"
              value={filter.account_id ?? ""}
              onChange={(e) =>
                setFilter({ ...filter, account_id: e.target.value || undefined })
              }
              className="rounded-md border border-slate-300 px-3 py-2 text-slate-900"
            >
              <option value="">すべて</option>
              {accountList.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </select>
          </div>

          <div className="space-y-1">
            <label htmlFor="filter-asset" className="block text-sm font-medium text-slate-700">
              銘柄
            </label>
            <select
              id="filter-asset"
              value={filter.asset_id ?? ""}
              onChange={(e) =>
                setFilter({ ...filter, asset_id: e.target.value || undefined })
              }
              className="rounded-md border border-slate-300 px-3 py-2 text-slate-900"
            >
              <option value="">すべて</option>
              {assetList.map((a) => (
                <option key={a.id} value={a.id}>{a.symbol} {a.name}</option>
              ))}
            </select>
          </div>

          <div className="space-y-1">
            <label htmlFor="filter-from" className="block text-sm font-medium text-slate-700">
              期間
            </label>
            <div className="flex items-center gap-2">
              <input
                id="filter-from"
                type="date"
                value={filter.from ?? ""}
                onChange={(e) => setFilter({ ...filter, from: e.target.value || undefined })}
                className="rounded-md border border-slate-300 px-3 py-2 text-slate-900"
              />
              <span className="text-slate-500">〜</span>
              <input
                type="date"
                aria-label="期間の終了日"
                value={filter.to ?? ""}
                onChange={(e) => setFilter({ ...filter, to: e.target.value || undefined })}
                className="rounded-md border border-slate-300 px-3 py-2 text-slate-900"
              />
            </div>
          </div>

          {filtered && (
            <button
              type="button"
              onClick={() => setFilter({})}
              className="rounded-md border border-slate-300 px-3 py-2 text-sm text-slate-700"
            >
              条件をクリア
            </button>
          )}
        </div>
      )}

      {transactions.length === 0 ? (
        filtered ? (
          <p className="text-sm text-slate-500">条件に一致する取引はありません。</p>
        ) : (
          <div className="rounded-lg border border-dashed border-slate-300 px-6 py-12 text-center">
            <p className="text-slate-900">まだ取引が登録されていません。</p>
            <p className="mt-1 text-sm text-slate-500">
              買付や売却を登録すると、保有状況や損益を確認できます。
            </p>
            <div className="mt-4">{addButton}</div>
          </div>
        )
      ) : (
        <div className="overflow-hidden rounded-lg border border-slate-200">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 text-left text-slate-600">
              <tr>
                <th scope="col" className="px-4 py-2 font-medium">約定日</th>
                <th scope="col" className="px-4 py-2 font-medium">口座</th>
                <th scope="col" className="px-4 py-2 font-medium">銘柄</th>
                <th scope="col" className="px-4 py-2 font-medium">種別</th>
                <th scope="col" className="px-4 py-2 text-right font-medium">数量</th>
                <th scope="col" className="px-4 py-2 text-right font-medium">単価</th>
                <th scope="col" className="px-4 py-2 text-right font-medium">手数料</th>
                <th scope="col" className="px-4 py-2 font-medium">メモ</th>
                <th scope="col" className="px-4 py-2"><span className="sr-only">操作</span></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200 text-slate-900">
              {transactions.map((t) => (
                <tr key={t.id}>
                  <td className="px-4 py-2 tabular-nums">{t.traded_at}</td>
                  <td className="px-4 py-2">{accountName.get(t.account_id) ?? "—"}</td>
                  <td className="px-4 py-2">{assetLabel.get(t.asset_id) ?? "—"}</td>
                  <td className="px-4 py-2">
                    <span
                      className={
                        t.kind === "buy" ? "text-slate-900" : "text-red-700"
                      }
                    >
                      {tradeKindLabels[t.kind]}
                    </span>
                  </td>
                  <td className="px-4 py-2 text-right tabular-nums">{t.quantity}</td>
                  <td className="px-4 py-2 text-right tabular-nums">{t.price}</td>
                  <td className="px-4 py-2 text-right tabular-nums">{t.fee}</td>
                  <td className="px-4 py-2 text-slate-500">{t.note ?? "—"}</td>
                  <td className="px-4 py-2 text-right">
                    <button
                      type="button"
                      onClick={() => setDeleting(t)}
                      className="rounded-md border border-slate-300 px-3 py-1 text-red-600"
                    >
                      削除
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <CreateTransactionDialog
        open={creating}
        onClose={() => setCreating(false)}
        accounts={accountList}
        assets={assetList}
      />
      {deleting && (
        <DeleteTransactionDialog
          key={deleting.id}
          transaction={deleting}
          label={`${deleting.traded_at} ${assetLabel.get(deleting.asset_id) ?? ""} ${tradeKindLabels[deleting.kind]} ${deleting.quantity}`}
          onClose={() => setDeleting(null)}
        />
      )}
    </section>
  );
}