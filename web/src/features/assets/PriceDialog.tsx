import { useRef, useEffect, useState } from "react";
import { ApiError } from "@/api/problem";
import { FormError } from "@/components/FormError";
import type { Asset, PriceItem } from "@/api/assets";
import { usePrices, useUpsertPrices } from "./queries";

type Props = {
  asset: Asset;
  onClose: () => void;
};

/** その日の日付を YYYY-MM-DD で返す（UTC ではなくローカル） */
function today() {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

type Row = { priced_on: string; price: string };

export function PriceDialog({ asset, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [rows, setRows] = useState<Row[]>([
    { priced_on: today(), price: "" },
  ]);
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState<number | null>(null);
  const history = usePrices(asset.id);
  const upsert = useUpsertPrices(asset.id);

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  const apiError = error instanceof ApiError ? error : null;
  const fieldErrors = apiError?.fieldErrors ?? {};
  const hasFieldError = Object.keys(fieldErrors).length > 0;

  function setRow(i: number, patch: Partial<Row>) {
    setRows(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(null);

    // 両方空の行は入力途中とみなして捨てる
    const prices: PriceItem[] = rows
      .filter((r) => r.priced_on.trim() !== "" || r.price.trim() !== "")
      .map((r) => ({ priced_on: r.priced_on, price: r.price.trim() }));

    if (prices.length === 0) {
      // 空配列を送るとサーバーが 400 を返す。手前で止める
      setError(new Error("価格を1件以上入力してください。"));
      return;
    }

    try {
      const res = await upsert.mutateAsync({ asset_id: asset.id, prices });
      setSaved(res.upserted);
      setRows([{ priced_on: today(), price: "" }]);
    } catch (e) {
      setError(e);
    }
  }

  return (
    <dialog
      ref={ref}
      onCancel={onClose}
      onClose={onClose}
      className="w-full max-w-lg rounded-lg p-0 backdrop:bg-slate-900/40"
    >
      <div className="space-y-4 p-6">
        <header>
          <h2 className="text-lg font-semibold text-slate-900">価格を登録</h2>
          <p className="mt-1 text-sm text-slate-500">
            {asset.symbol} {asset.name}（{asset.currency} /{" "}
            {asset.price_unit}口あたり）
          </p>
        </header>

        <form onSubmit={handleSubmit} noValidate className="space-y-3">
          {!hasFieldError && <FormError error={error} />}
          {saved !== null && (
            <p className="text-sm text-green-700">
              {saved}件を登録しました。
            </p>
          )}

          {rows.map((row, i) => (
            <div key={i} className="flex items-end gap-2">
              <div className="space-y-1">
                <label
                  htmlFor={`priced-on-${i}`}
                  className="block text-sm font-medium text-slate-700"
                >
                  日付
                </label>
                <input
                  id={`priced-on-${i}`}
                  type="date"
                  value={row.priced_on}
                  max={today()}
                  onChange={(e) => setRow(i, { priced_on: e.target.value })}
                  className="rounded-md border border-slate-300 px-3 py-2 text-slate-900 outline-none focus:ring-2 focus:ring-slate-300"
                />
              </div>
              <div className="flex-1 space-y-1">
                <label
                  htmlFor={`price-${i}`}
                  className="block text-sm font-medium text-slate-700"
                >
                  価格
                </label>
                {/* Decimal を文字列のまま送るため type="number" は使わない */}
                <input
                  id={`price-${i}`}
                  type="text"
                  inputMode="decimal"
                  value={row.price}
                  onChange={(e) => setRow(i, { price: e.target.value })}
                  className="w-full rounded-md border border-slate-300 px-3 py-2 text-right tabular-nums text-slate-900 outline-none focus:ring-2 focus:ring-slate-300"
                />
              </div>
              {rows.length > 1 && (
                <button
                  type="button"
                  onClick={() => setRows(rows.filter((_, j) => j !== i))}
                  className="rounded-md border border-slate-300 px-3 py-2 text-sm text-slate-600"
                >
                  削除
                </button>
              )}
            </div>
          ))}

          {(fieldErrors.priced_on || fieldErrors.price) && (
            <p className="text-sm text-red-600">
              {fieldErrors.priced_on ?? fieldErrors.price}
            </p>
          )}

          <button
            type="button"
            onClick={() => setRows([...rows, { priced_on: "", price: "" }])}
            className="text-sm text-slate-600 underline"
          >
            日付を追加
          </button>

          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={upsert.isPending}
              className="rounded-md border border-slate-300 px-4 py-2 text-sm text-slate-700 disabled:opacity-50"
            >
              閉じる
            </button>
            <button
              type="submit"
              disabled={upsert.isPending}
              className="rounded-md bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-50"
            >
              {upsert.isPending ? "登録中…" : "登録"}
            </button>
          </div>
        </form>

        <section className="space-y-2 border-t border-slate-200 pt-4">
          <h3 className="text-sm font-medium text-slate-700">登録済みの価格</h3>
          {history.isPending ? (
            <p className="text-sm text-slate-500">読み込み中…</p>
          ) : history.isError ? (
            <p className="text-sm text-red-600">
              価格履歴の取得に失敗しました。
            </p>
          ) : history.data.length === 0 ? (
            <p className="text-sm text-slate-500">まだ登録がありません。</p>
          ) : (
            <div className="max-h-48 overflow-y-auto">
              <table className="w-full text-sm">
                <thead className="text-left text-slate-600">
                  <tr>
                    <th scope="col" className="py-1 font-medium">日付</th>
                    <th scope="col" className="py-1 text-right font-medium">価格</th>
                    <th scope="col" className="py-1 font-medium">出所</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100 text-slate-900">
                  {[...history.data].reverse().map((p) => (
                    <tr key={p.priced_on}>
                      <td className="py-1">{p.priced_on}</td>
                      <td className="py-1 text-right tabular-nums">{p.price}</td>
                      <td className="py-1 text-slate-500">{p.source}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>
    </dialog>
  );
}