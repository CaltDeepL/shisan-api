import { useMutation, useQueryClient } from "@tanstack/react-query";
import { dryRunImport, runImport } from "../../api/import";

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
      void qc.invalidateQueries({ queryKey: ["transactions"] });
      void qc.invalidateQueries({ queryKey: ["holdings"] });
      void qc.invalidateQueries({ queryKey: ["analytics"] });
    },
  });
}