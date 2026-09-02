import { useState } from "react";
import { AssetHistoryChart } from "./AssetHistoryChart";
import type { HistoryGroupBy } from "./AssetHistoryChart";
import { AllocationChart } from "./AllocationChart";
import type { AllocationGroupBy } from "./AllocationChart";
import { resolveGranularity, resolveRange } from "./format";
import type { PeriodPreset } from "./format";
import { useAllocation, useAssetHistory } from "./queries";

export function AnalyticsPage() {
  const [preset, setPreset] = useState<PeriodPreset>("6m");
  const [historyGroupBy, setHistoryGroupBy] = useState<HistoryGroupBy>("none");
  const [allocGroupBy, setAllocGroupBy] = useState<AllocationGroupBy>("asset_class");

  const { from, to } = resolveRange(preset);
  const history = useAssetHistory({
    from,
    to,
    granularity: resolveGranularity(preset),
    group_by: historyGroupBy,
  });
  const allocation = useAllocation({ group_by: allocGroupBy });

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-6 p-4">
      <h1 className="text-xl font-semibold text-slate-900">分析</h1>

      <AssetHistoryChart
        result={history.data ?? null}
        loading={history.isPending}
        error={history.isError ? history.error.message : null}
        preset={preset}
        onPresetChange={setPreset}
        groupBy={historyGroupBy}
        onGroupByChange={setHistoryGroupBy}
        onRetry={() => void history.refetch()}
      />

      <AllocationChart
        result={allocation.data ?? null}
        loading={allocation.isPending}
        error={allocation.isError ? allocation.error.message : null}
        groupBy={allocGroupBy}
        onGroupByChange={setAllocGroupBy}
        onRetry={() => void allocation.refetch()}
      />
    </div>
  );
}
