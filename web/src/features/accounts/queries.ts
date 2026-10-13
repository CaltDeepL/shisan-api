import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createAccount,
  deleteAccount,
  listAccounts,
  updateAccount,
  type CreateAccountRequest,
  type UpdateAccountRequest,
} from "@/api/accounts";
import { invalidateQueryRoots, queryKeys } from "@/lib/queryKeys";

export function useAccounts() {
  return useQuery({ queryKey: queryKeys.accounts, queryFn: listAccounts });
}

export function useCreateAccount() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAccountRequest) => createAccount(body),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.accounts,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}

export function useUpdateAccount() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateAccountRequest }) =>
      updateAccount(id, body),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.accounts,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}

export function useDeleteAccount() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteAccount(id),
    onSuccess: () =>
      invalidateQueryRoots(
        qc,
        queryKeys.accounts,
        queryKeys.holdings,
        queryKeys.analytics,
      ),
  });
}
