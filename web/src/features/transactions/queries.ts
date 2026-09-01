import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createTransaction,
  deleteTransaction,
  listTransactions,
  type CreateTransaction,
  type TransactionFilter,
} from "@/api/transactions";

export const transactionsKey = ["transactions"] as const;

export function useTransactions(filter: TransactionFilter) {
  return useQuery({
    queryKey: [...transactionsKey, filter] as const,
    queryFn: () => listTransactions(filter),
  });
}

export function useCreateTransaction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTransaction) => createTransaction(body),
    onSuccess: () => qc.invalidateQueries({ queryKey: transactionsKey }),
  });
}

export function useDeleteTransaction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteTransaction(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: transactionsKey }),
  });
}