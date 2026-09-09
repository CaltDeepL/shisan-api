import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createTransaction,
  deleteTransaction,
  listTransactions,
  type CreateTransaction,
  type TransactionFilter,
} from "@/api/transactions";
import { invalidateQueryRoots, queryKeys } from "@/lib/queryKeys";

export function useTransactions(filter: TransactionFilter) {
  return useQuery({
    queryKey: [...queryKeys.transactions, filter] as const,
    queryFn: () => listTransactions(filter),
  });
}

export function useCreateTransaction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTransaction) => createTransaction(body),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.transactions,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}

export function useDeleteTransaction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteTransaction(id),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.transactions,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}
