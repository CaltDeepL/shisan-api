import { useHoldings } from "@/features/holdings/queries";
import { isPriced, type HoldingItem } from "@/api/holdings";
import { SummarySection } from "./SummarySection";
import { useState } from "react";
import { useAccounts } from "@/features/accounts/queries";;
import { ApiError } from "@/api/problem";
import type { HoldingsResponse } from "@/api/holdings";
import {
  formatMoney,
  formatQuantity,
  formatRatioAsPercent,
  formatSignedMoney,
  formatUnitPrice,
  pnlClass,
} from "@/features/holdings/format";

export function HoldingsPage() {
  const [accountId, setAccountId] = useState("");
  const [includeClosed, setIncludeClosed] = useState(false);

  const accounts = useAccounts();
  const { data, isPending, isError, error } = useHoldings({
    // 空文字は「全口座」。undefined にしないとクエリに ?account_id= が付く
    account_id: accountId || undefined,
    include_closed: includeClosed,
  });

  return (
    <div className="p-6">
      <h1 className="mb-4 text-xl font-semibold">保有一覧</h1>

      <div className="mb-4 flex flex-wrap items-center gap-4">
        <label className="flex items-center gap-2 text-sm">
          <span className="text-slate-600">口座</span>
          <select
            value={accountId}
            onChange={(e) => setAccountId(e.target.value)}
            className="rounded border border-slate-300 px-2 py-1 text-sm"
          >
            <option value="">すべて</option>
            {accounts.data?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={includeClosed}
            onChange={(e) => setIncludeClosed(e.target.checked)}
            className="rounded border-slate-300"
          />
          全売却済みも表示
        </label>
      </div>

      <HoldingsContent
        data={data}
        isPending={isPending}
        isError={isError}
        error={error}
      />
    </div>
  );
}
function HoldingsContent({
  data,
  isPending,
  isError,
  error,
}: {
  data: HoldingsResponse | undefined;
  isPending: boolean;
  isError: boolean;
  error: Error | null;
}) {
  if (isPending) return <p className="text-slate-500">読み込み中…</p>;

  if (isError) {
    // 404 は「指定した口座が存在しない」。他ユーザーの口座を指したときも同じ
    const message =
      error instanceof ApiError && error.status === 404
        ? "指定した口座が見つかりません。口座を「すべて」に戻してください。"
        : `保有一覧を取得できませんでした：${error?.message ?? ""}`;
    return <p className="text-rose-600">{message}</p>;
  }

  if (!data) return null;

  const { holdings, summary } = data;
  const unpricedCount = summary.unpriced_count;

  if (holdings.length === 0) {
    return (
      <p className="rounded border border-slate-200 p-8 text-center text-slate-500">
        該当する保有がありません。
      </p>
    );
  }

  return (
    <>
      <SummarySection totals={summary.totals} byAccount={summary.by_account} />
      {unpricedCount > 0 && (
        <p className="mb-3 rounded bg-amber-50 px-3 py-2 text-sm text-amber-800">
          価格未登録の銘柄が {unpricedCount} 件あります。
          評価額・評価損益の合計には含まれていません。
        </p>
      )}
      <HoldingsTable holdings={holdings} />
    </>
  );
}

function HoldingsTable({ holdings }: { holdings: HoldingItem[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-slate-300 text-left text-slate-600">
            <th className="px-3 py-2">口座</th>
            <th className="px-3 py-2">シンボル</th>
            <th className="px-3 py-2">銘柄名</th>
            <th className="px-3 py-2 text-right">数量</th>
            <th className="px-3 py-2 text-right">平均取得単価</th>
            <th className="px-3 py-2 text-right">簿価</th>
            <th className="px-3 py-2 text-right">現在値</th>
            <th className="px-3 py-2 text-right">評価額</th>
            <th className="px-3 py-2 text-right">評価損益</th>
            <th className="px-3 py-2 text-right">騰落率</th>
          </tr>
        </thead>
        <tbody>
          {holdings.map((h) => {
            const priced = isPriced(h);
            return (
              <tr
                key={`${h.account_id}:${h.asset_id}`}
                className="border-b border-slate-100"
              >
                <td className="px-3 py-2">{h.account_name}</td>
                <td className="px-3 py-2 font-mono">{h.symbol}</td>
                <td className="px-3 py-2">
                  {h.name}
                  {!priced && (
                    <span className="ml-2 rounded bg-amber-100 px-1.5 py-0.5 text-xs text-amber-800">
                      価格未登録
                    </span>
                  )}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {formatQuantity(h.quantity)}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {formatUnitPrice(h.avg_cost)}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {formatMoney(h.book_value)}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {formatUnitPrice(h.price)}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {formatMoney(h.market_value)}
                </td>
                <td
                  className={`px-3 py-2 text-right tabular-nums ${pnlClass(h.unrealized_pnl)}`}
                >
                  {formatSignedMoney(h.unrealized_pnl)}
                </td>
                <td
                  className={`px-3 py-2 text-right tabular-nums ${pnlClass(h.unrealized_pnl_rate)}`}
                >
                  {formatRatioAsPercent(h.unrealized_pnl_rate)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}