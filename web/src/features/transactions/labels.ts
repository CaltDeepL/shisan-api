import type { TradeKind } from "@/api/transactions";

export const tradeKindLabels: Record<TradeKind, string> = {
  buy: "買付",
  sell: "売却",
};