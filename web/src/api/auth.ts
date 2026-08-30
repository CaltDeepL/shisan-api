import type { components } from "./schema";
import { apiFetch } from "./client";

export type Credentials = components["schemas"]["Credentials"];
export type TokenResponse = components["schemas"]["TokenResponse"];
export type MeResponse = components["schemas"]["MeResponse"];

export function register(body: Credentials) {
  return apiFetch<TokenResponse>("/auth/register", {
    method: "POST",
    body,
    auth: false,
  });
}

export function login(body: Credentials) {
  return apiFetch<TokenResponse>("/auth/login", {
    method: "POST",
    body,
    auth: false,
  });
}

export function fetchMe() {
  return apiFetch<MeResponse>("/me");
}
