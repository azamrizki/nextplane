// Marks work item keys such as WEB-12 in the editor so they can show a live card.
import { Extension } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

export const refsKey = new PluginKey<DecorationSet>("work-item-refs");

/** A pattern for keys of the given project identifiers, or null when there are none. */
export function refPattern(identifiers: string[]): RegExp | null {
  const list = identifiers.filter((i) => /^[A-Z][A-Z0-9]{0,11}$/.test(i));
  if (!list.length) return null;
  return new RegExp(`\\b(?:${list.join("|")})-[1-9]\\d{0,8}\\b`, "g");
}

export function findRefs(text: string, re: RegExp | null): { from: number; to: number; key: string }[] {
  if (!re) return [];
  const out = [];
  re.lastIndex = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) out.push({ from: m.index, to: m.index + m[0].length, key: m[0] });
  return out;
}

function build(doc: PMNode, re: RegExp | null): DecorationSet {
  if (!re) return DecorationSet.empty;
  const decos: Decoration[] = [];
  doc.descendants((node, pos) => {
    if (node.type.name === "codeBlock") return false;
    if (!node.isText || !node.text || node.marks.some((m) => m.type.name === "code")) return;
    for (const r of findRefs(node.text, re)) decos.push(Decoration.inline(pos + r.from, pos + r.to, { class: "wi-ref", "data-key": r.key }));
  });
  return DecorationSet.create(doc, decos);
}

export const WorkItemRefs = Extension.create<{ identifiers: () => string[] }>({
  name: "workItemRefs",
  addOptions() {
    return { identifiers: () => [] };
  },
  addProseMirrorPlugins() {
    const opts = this.options;
    return [
      new Plugin<DecorationSet>({
        key: refsKey,
        state: {
          init: (_, state) => build(state.doc, refPattern(opts.identifiers())),
          apply: (tr, old) => (tr.docChanged || tr.getMeta(refsKey) ? build(tr.doc, refPattern(opts.identifiers())) : old),
        },
        props: { decorations: (state) => refsKey.getState(state) },
      }),
    ];
  },
});
