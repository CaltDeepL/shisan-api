import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createAccount,
  deleteAccount,
  listAccounts,
  updateAccount,
  type CreateAccountRequest,
  type UpdateAccountRequest,
} from "@/api/accounts";

export const accountsKey = ["accounts"] as const;

export function useAccounts() {
  return useQuery({ queryKey: accountsKey, queryFn: listAccounts });
}

export function useCreateAccount() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAccountRequest) => createAccount(body),
    onSuccess: () => qc.invalidateQueries({ queryKey: accountsKey }),
  });
}

export function useUpdateAccount(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateAccountRequest) => updateAccount(id, body),
    onSuccess: () => qc.invalidateQueries({ queryKey: accountsKey }),
  });
}

export function useDeleteAccount() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteAccount(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: accountsKey }),
  });
}
