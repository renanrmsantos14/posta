// Pilha de desfazer (Ctrl+Z): cada ação reversível empilha uma função; só a última é desfeita por vez.
interface Entry { label: string; run: () => void | Promise<void> }
const stack: Entry[] = [];

export function pushUndo(label: string, run: Entry["run"]) {
  stack.push({ label, run });
  if (stack.length > 20) stack.shift();
}

/** Tira uma entrada que já foi desfeita por outro caminho (ex.: botão "Desfazer" do aviso). */
export function dropUndo(run: Entry["run"]) {
  const i = stack.findIndex((e) => e.run === run);
  if (i >= 0) stack.splice(i, 1);
}

/** Desfaz a última ação; devolve o rótulo, ou null se não havia nada. */
export async function popUndo(): Promise<string | null> {
  const e = stack.pop();
  if (!e) return null;
  await e.run();
  return e.label;
}
