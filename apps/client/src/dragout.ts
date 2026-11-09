// Arrasto nativo para fora do app (plugin tauri-plugin-drag): os arquivos temporários são
// preparados no backend e o sistema assume o arrasto, que aparece como .eml/anexo no destino.
import { isTauri, type DragFiles } from "./api";

/** Só faz sentido no app instalado; no navegador (modo mock) não existe arrasto nativo. */
export const canDragOut = isTauri;

/** Prepara os arquivos e inicia o arrasto nativo, se o botão do mouse continuar pressionado. */
export async function dragOut(prepare: () => Promise<DragFiles>, released: () => boolean): Promise<void> {
  const { files, icon } = await prepare();
  if (released() || !files.length) return;
  const { startDrag } = await import("@crabnebula/tauri-plugin-drag");
  await startDrag({ item: files, icon });
}
