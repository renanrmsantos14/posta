// Camadas do layout: AppShell -> RouteLayout -> PageLayout -> primitivas (Stack, Grid, Cluster, Split).
import type { CSSProperties, HTMLAttributes, ReactNode, Ref } from "react";

type Div = HTMLAttributes<HTMLElement>;
const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(" ") || undefined;

/** Raiz da janela: área "top" (barras) sobre a área "route" (as três colunas). */
export function AppShell({ shellRef, className, children, ...rest }: Div & { shellRef?: Ref<HTMLDivElement> }) {
  return <div ref={shellRef} className={cx("app", className)} {...rest}>{children}</div>;
}

/** Corpo da rota: grade com as áreas nomeadas rail / list / reader. */
export function RouteLayout({ children, className, ...rest }: Div) {
  return <div className={cx("route", className)} {...rest}>{children}</div>;
}

/** Uma coluna da rota, ligada à sua área da grade. */
export function PageLayout({ area, as: Tag = "section", className, children, ...rest }: Div & { area: "rail" | "list" | "reader"; as?: "section" | "main" | "aside" }) {
  return <Tag className={cx("page", area === "rail" ? "rail" : area === "list" ? "list-pane" : "reader", className)} data-area={area} {...rest}>{children}</Tag>;
}

const gap = (g?: number): CSSProperties | undefined => (g === undefined ? undefined : ({ "--gap": `${g}px` } as CSSProperties));
type P = Div & { gap?: number; children?: ReactNode };

export const Stack = ({ gap: g, className, children, ...rest }: P) => <div className={cx("stack", className)} style={gap(g)} {...rest}>{children}</div>;
export const Cluster = ({ gap: g, className, children, ...rest }: P) => <div className={cx("cluster", className)} style={gap(g)} {...rest}>{children}</div>;
export const Grid = ({ gap: g, className, children, ...rest }: P) => <div className={cx("grid", className)} style={gap(g)} {...rest}>{children}</div>;
/** Dois blocos lado a lado; o filho com `.grow` ocupa o resto. */
export const Split = ({ gap: g, className, children, ...rest }: P) => <div className={cx("split", className)} style={gap(g)} {...rest}>{children}</div>;
