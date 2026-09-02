import type { AccountSummary, Totals } from "@/api/holdings";
import { accountTypeLabels } from "@/features/accounts/labels";
import {
  formatMoney,
  formatRatioAsPercent,
  formatSignedMoney,
  pnlClass,
} from "./format";

export function SummarySection({
  totals,
  byAccount,
}: {
  totals: Totals[];
  byAccount: AccountSummary[];
}) {
  if (totals.length === 0) return null;

  return (
    <section className="mb-6 space-y-4">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {totals.map((t) => (
          <TotalsCard key={t.currency} totals={t} />
        ))}
      </div>

      {byAccount.length > 1 && (
        <details className="rounded border border-slate-200 bg-white">
          <summary className="cursor-pointer px-4 py-2 text-sm text-slate-600">
            口座別の内訳
          </summary>
          <div className="space-y-3 border-t border-slate-100 p-4">
            {byAccount.map((a) => (
              <div key={a.account_id}>
                <p className="mb-2 text-sm font-medium text-slate-800">
                  {a.account_name}
                  <span className="ml-2 text-xs text-slate-500">
                    {accountTypeLabels[a.account_type]}
                  </span>
                </p>
                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                  {a.totals.map((t) => (
                    <TotalsCard key={t.currency} totals={t} compact />
                  ))}
                </div>
              </div>
            ))}
          </div>
        </details>
      )}
    </section>
  );
}

function TotalsCard({
  totals: t,
  compact = false,
}: {
  totals: Totals;
  compact?: boolean;
}) {
  return (
    <div
      className={`rounded border border-slate-200 bg-white ${compact ? "p-3" : "p-4"}`}
    >
      <div className="mb-2 flex items-baseline justify-between">
        <span className="text-xs font-medium text-slate-500">{t.currency}</span>
        {t.unpriced_count > 0 && (
          <span className="text-xs text-amber-700">
            未評価 {t.unpriced_count} 件
          </span>
        )}
      </div>

      <dl className="space-y-1 text-sm">
        <Row label="評価額" value={formatMoney(t.market_value)} />
        <Row label="簿価" value={formatMoney(t.book_value)} hint="未評価分を含む" />
        <Row
          label="評価損益"
          value={formatSignedMoney(t.unrealized_pnl)}
          className={pnlClass(t.unrealized_pnl)}
        />
        <Row
          label="騰落率"
          value={formatRatioAsPercent(t.unrealized_pnl_rate)}
          className={pnlClass(t.unrealized_pnl_rate)}
        />
        <Row label="実現損益" value={formatSignedMoney(t.realized_pnl)} className={pnlClass(t.realized_pnl)} />
      </dl>
    </div>
  );
}

function Row({
  label,
  value,
  className = "",
  hint,
}: {
  label: string;
  value: string;
  className?: string;
  hint?: string;
}) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <dt className="text-slate-500">
        {label}
        {hint && <span className="ml-1 text-xs text-slate-400">（{hint}）</span>}
      </dt>
      <dd className={`tabular-nums ${className}`}>{value}</dd>
    </div>
  );
}