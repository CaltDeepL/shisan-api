import { useMutation, useQueryClient } from "@tanstack/react-query";
import { dryRunImport, runImport } from "../../api/import";
import { invalidateQueryRoots, queryKeys } from "@/lib/queryKeys";

export function useDryRunImport() {
  return useMutation({ mutationFn: dryRunImport });
}

export function useRunImport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: runImport,
    onSuccess: (outcome) => {
      // rejected(422) では何も入っていないので無効化しない
      if (outcome.kind !== "inserted") return;
      if (outcome.result.inserted === 0) return;
      return invalidateQueryRoots(
        qc,
        queryKeys.transactions,
        queryKeys.holdings,
        queryKeys.analytics,
      );
    },
  });
}
