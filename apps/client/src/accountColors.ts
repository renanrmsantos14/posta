// Cor da conta na barra lateral: guardada na configuração `color:<id da conta>`; sem ela vale o verde de sempre.
export const ACCOUNT_COLORS = [
  { id: "blue", label: "Azul" },
  { id: "green", label: "Verde" },
  { id: "purple", label: "Roxo" },
  { id: "orange", label: "Laranja" },
  { id: "yellow", label: "Amarelo" },
  { id: "red", label: "Vermelho" },
] as const;

export const colorKey = (accountId: string) => `color:${accountId}`;
