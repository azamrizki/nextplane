// The live page editor: Tiptap bound to a shared Yjs document over the page's socket, with
// named cursors and a presence list. The server saves the document and builds its HTML.
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Image from "@tiptap/extension-image";
import Collaboration from "@tiptap/extension-collaboration";
import CollaborationCaret from "@tiptap/extension-collaboration-caret";
import { useEffect, useImperativeHandle, useMemo, useRef, useState, type Ref } from "react";
import { WebsocketProvider } from "y-websocket";
import * as Y from "yjs";
import { useI18n } from "../i18n";
import { Toolbar } from "./Editor";
import { WorkItemRefs, refsKey } from "./WorkItemRefs";

export interface Presence {
  clientId: number;
  name: string;
  color: string;
}

export interface CollabHandle {
  setContent: (html: string) => void;
}

const COLORS = ["#0e7c6b", "#6e56cf", "#d6409f", "#e5484d", "#f76b15", "#ad7f58", "#3e63dd", "#12a594"];
export function colorFor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
  return COLORS[Math.abs(h) % COLORS.length]!;
}

export function CollabEditor({
  pageId,
  editable,
  me,
  seedHtml,
  onPresence,
  onUpload,
  identifiers,
  onCreateItem,
  handle,
}: {
  pageId: string;
  editable: boolean;
  me: { id: string; name: string };
  seedHtml?: string;
  onPresence?: (p: Presence[]) => void;
  onUpload?: (f: File) => Promise<string>;
  /** Project identifiers whose keys (WEB-12) become live references. */
  identifiers?: string[];
  /** Creates a work item titled with the selected text; returns its key. */
  onCreateItem?: (title: string) => Promise<string>;
  handle?: Ref<CollabHandle>;
}) {
  const { t } = useI18n();
  const [status, setStatus] = useState<"connecting" | "connected" | "disconnected">("connecting");
  const { doc, provider } = useMemo(() => {
    const doc = new Y.Doc();
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const provider = new WebsocketProvider(`${proto}://${location.host}/api/v1/collab`, pageId, doc);
    return { doc, provider };
  }, [pageId]);
  useEffect(() => () => {
    provider.destroy();
    doc.destroy();
  }, [provider, doc]);

  const uploadRef = useRef(onUpload);
  uploadRef.current = onUpload;
  const idents = useRef<string[]>(identifiers ?? []);
  idents.current = identifiers ?? [];
  const [hasSel, setHasSel] = useState(false);
  const [creating, setCreating] = useState(false);

  const editor = useEditor(
    {
      extensions: [
        StarterKit.configure({ undoRedo: false, link: { openOnClick: false, autolink: true, protocols: ["http", "https", "mailto"] } }),
        Placeholder.configure({ placeholder: t("pg.bodyPlaceholder") }),
        TaskList,
        TaskItem.configure({ nested: true }),
        Image,
        WorkItemRefs.configure({ identifiers: () => idents.current }),
        Collaboration.configure({ document: doc }),
        CollaborationCaret.configure({ provider, user: { name: me.name, color: colorFor(me.id) } }),
      ],
      editable,
      editorProps: {
        attributes: { class: "prose page-prose", role: "textbox", "aria-multiline": "true", "aria-label": t("pg.body") },
        handlePaste: (_v, e) => {
          const up = uploadRef.current;
          const files = Array.from(e.clipboardData?.files ?? []).filter((f) => /^image\/(png|jpeg|gif|webp)$/.test(f.type));
          if (!up || !files.length) return false;
          for (const f of files) up(f).then((src) => editorRef.current?.chain().focus().setImage({ src, alt: f.name }).run(), () => {});
          return true;
        },
      },
    },
    [doc, provider],
  );
  const editorRef = useRef(editor);
  editorRef.current = editor;
  useEffect(() => editor?.setEditable(editable), [editor, editable]);
  // Identifiers arrive after the editor: mark references again.
  const identKey = (identifiers ?? []).join(",");
  useEffect(() => {
    if (editor && !editor.isDestroyed) editor.view.dispatch(editor.state.tr.setMeta(refsKey, true));
  }, [editor, identKey]);
  useEffect(() => {
    if (!editor) return;
    const onSel = () => setHasSel(!editor.state.selection.empty);
    editor.on("selectionUpdate", onSel);
    return () => {
      editor.off("selectionUpdate", onSel);
    };
  }, [editor]);
  const createItem = async () => {
    if (!editor || !onCreateItem) return;
    const { from, to } = editor.state.selection;
    const text = editor.state.doc.textBetween(from, to, " ");
    if (!text.trim()) return;
    setCreating(true);
    try {
      const key = await onCreateItem(text);
      // The key before the text turns into a live reference.
      editor.chain().focus().insertContentAt(from, `${key} `).run();
    } finally {
      setCreating(false);
    }
  };
  useImperativeHandle(handle, () => ({ setContent: (html: string) => editor?.commands.setContent(html) }), [editor]);

  useEffect(() => {
    const onStatus = (e: { status: "connecting" | "connected" | "disconnected" }) => setStatus(e.status);
    provider.on("status", onStatus);
    // A page created with content but never opened in an editor: seed it once.
    const onSync = (synced: boolean) => {
      // Only when nobody else is here, so two people opening it at once do not both seed it.
      const alone = [...provider.awareness.getStates().keys()].every((id) => id === doc.clientID);
      if (synced && editable && seedHtml && alone && editorRef.current?.isEmpty) editorRef.current.commands.setContent(seedHtml);
    };
    provider.on("sync", onSync);
    const aw = provider.awareness;
    const onChange = () => {
      const list: Presence[] = [];
      aw.getStates().forEach((st, clientId) => {
        const u = (st as { user?: { name: string; color: string } }).user;
        if (u) list.push({ clientId, name: u.name, color: u.color });
      });
      onPresenceRef.current?.(list);
    };
    aw.on("change", onChange);
    onChange(); // my own state was set before this listener existed
    return () => {
      provider.off("status", onStatus);
      provider.off("sync", onSync);
      aw.off("change", onChange);
    };
  }, [provider, doc, editable, seedHtml]);
  const onPresenceRef = useRef(onPresence);
  onPresenceRef.current = onPresence;

  return (
    <div className="collab">
      {status !== "connected" ? <div className="faint small collab-status">{status === "connecting" ? t("pg.connecting") : t("pg.offline")}</div> : null}
      <div className="editor page-editor">
        {editor && editable ? (
          <div className="collab-tools">
            <Toolbar editor={editor} />
            {onCreateItem ? (
              <button
                type="button"
                className="btn btn-sm"
                title={t("pg.toWorkItemHint")}
                disabled={!hasSel || creating}
                onMouseDown={(e) => e.preventDefault()}
                onClick={createItem}
              >
                {t("pg.toWorkItem")}
              </button>
            ) : null}
          </div>
        ) : null}
        <EditorContent editor={editor} />
      </div>
    </div>
  );
}
