import { layerHost, placeNear } from "./Floating";
import { EditorContent, useEditor, type Editor as TiptapEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Image from "@tiptap/extension-image";
import Mention from "@tiptap/extension-mention";
import type { SuggestionProps, SuggestionKeyDownProps } from "@tiptap/suggestion";
import { useEffect, useImperativeHandle, useRef, type Ref } from "react";

export interface EditorValue {
  json: unknown;
  html: string;
}

export interface MentionPerson {
  id: string;
  label: string;
}

export interface EditorHandle {
  clear: () => void;
  focus: () => void;
  isEmpty: () => boolean;
  value: () => EditorValue | null;
  /** Inserts HTML at the cursor (or at the end). */
  insert: (html: string) => void;
}

/** The rich-text editor (Tiptap). Calls `onChange` at most every `debounce` ms while typing,
 * and once more on blur. `people` turns on @mentions; `onUpload` turns on pasting and
 * dropping images. */
export function Editor({
  initial,
  initialHtml,
  onChange,
  editable = true,
  placeholder,
  debounce = 800,
  autoFocus,
  ariaLabel,
  minHeight = 120,
  people,
  onUpload,
  onSubmit,
  handle,
}: {
  initial?: unknown;
  initialHtml?: string;
  onChange?: (v: EditorValue) => void;
  editable?: boolean;
  placeholder?: string;
  debounce?: number;
  autoFocus?: boolean;
  ariaLabel?: string;
  minHeight?: number;
  people?: MentionPerson[];
  onUpload?: (f: File) => Promise<string>;
  /** Ctrl/⌘+Enter */
  onSubmit?: () => void;
  handle?: Ref<EditorHandle>;
}) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cb = useRef(onChange);
  cb.current = onChange;
  const peopleRef = useRef(people ?? []);
  peopleRef.current = people ?? [];
  const uploadRef = useRef(onUpload);
  uploadRef.current = onUpload;
  const submitRef = useRef(onSubmit);
  submitRef.current = onSubmit;

  const insertImages = (editor: TiptapEditor, files: File[], pos?: number) => {
    const up = uploadRef.current;
    if (!up) return false;
    const images = files.filter((f) => /^image\/(png|jpeg|gif|webp)$/.test(f.type));
    if (!images.length) return false;
    for (const f of images) {
      up(f).then(
        (src) => {
          const chain = editor.chain().focus();
          (pos !== undefined ? chain.insertContentAt(pos, { type: "image", attrs: { src, alt: f.name } }) : chain.setImage({ src, alt: f.name })).run();
        },
        () => {},
      );
    }
    return true;
  };

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ link: { openOnClick: false, autolink: true, protocols: ["http", "https", "mailto"] } }),
      Placeholder.configure({ placeholder: placeholder ?? "" }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Image.configure({ inline: false }),
      Mention.configure({
        HTMLAttributes: { class: "mention" },
        renderText: ({ node }) => `@${node.attrs.label ?? node.attrs.id}`,
        suggestion: {
          items: ({ query }) => {
            const q = query.toLowerCase();
            return peopleRef.current.filter((p) => p.label.toLowerCase().includes(q)).slice(0, 8);
          },
          render: mentionMenu,
        },
      }),
    ],
    content: (initial as object | undefined) ?? initialHtml ?? "",
    editable,
    autofocus: autoFocus ? "end" : false,
    editorProps: {
      attributes: { class: "prose", "aria-label": ariaLabel ?? "", role: "textbox", "aria-multiline": "true" },
      handlePaste: (_view, event) => {
        const files = Array.from(event.clipboardData?.files ?? []);
        return files.length > 0 && !!editorRef.current && insertImages(editorRef.current, files);
      },
      handleDrop: (view, event) => {
        const files = Array.from((event as DragEvent).dataTransfer?.files ?? []);
        if (!files.length || !editorRef.current) return false;
        const pos = view.posAtCoords({ left: (event as DragEvent).clientX, top: (event as DragEvent).clientY })?.pos;
        return insertImages(editorRef.current, files, pos);
      },
      handleKeyDown: (_view, event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && submitRef.current) {
          submitRef.current();
          return true;
        }
        return false;
      },
    },
    onUpdate: ({ editor }) => {
      if (!cb.current || !editor.isEditable) return;
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => cb.current?.({ json: editor.getJSON(), html: editor.getHTML() }), debounce);
    },
    onBlur: ({ editor }) => {
      if (!timer.current || !cb.current) return;
      clearTimeout(timer.current);
      timer.current = null;
      cb.current({ json: editor.getJSON(), html: editor.getHTML() });
    },
  });
  const editorRef = useRef<TiptapEditor | null>(null);
  editorRef.current = editor;
  useImperativeHandle(handle, () => ({
    clear: () => editor?.commands.clearContent(true),
    focus: () => editor?.commands.focus(),
    isEmpty: () => !editor || editor.isEmpty,
    value: () => (editor ? { json: editor.getJSON(), html: editor.getHTML() } : null),
    insert: (html: string) => void editor?.chain().focus().insertContent(html).run(),
  }), [editor]);
  // Without the second argument Tiptap emits an update, which would save unchanged text.
  useEffect(() => editor?.setEditable(editable, false), [editor, editable]);
  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);
  return (
    <div className="editor" style={{ minHeight }}>
      {editor && editable ? <Toolbar editor={editor} /> : null}
      <EditorContent editor={editor} />
    </div>
  );
}

/** A small DOM menu for @mention suggestions (no extra dependency). */
function mentionMenu() {
  let el: HTMLDivElement | null = null;
  let props: SuggestionProps<MentionPerson> | null = null;
  let index = 0;
  const draw = () => {
    if (!el || !props) return;
    el.replaceChildren();
    if (!props.items.length) {
      el.style.display = "none";
      return;
    }
    el.style.display = "";
    props.items.forEach((p, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "menu-item" + (i === index ? " focused" : "");
      b.setAttribute("role", "option");
      b.textContent = p.label;
      b.onmousedown = (e) => {
        e.preventDefault();
        props?.command({ id: p.id, label: p.label });
      };
      el!.appendChild(b);
    });
    // Next to the caret, kept on screen (flips up near the bottom), as menus are.
    const rect = props.clientRect?.();
    if (rect) placeNear(rect, el, "start", false);
  };
  return {
    onStart: (p: SuggestionProps<MentionPerson>) => {
      props = p;
      index = 0;
      el = document.createElement("div");
      el.className = "menu mention-menu floating";
      el.setAttribute("role", "listbox");
      el.style.position = "fixed";
      // Inside a modal dialog the menu must live in the dialog to be seen above it.
      layerHost(p.editor.view.dom).appendChild(el);
      draw();
    },
    onUpdate: (p: SuggestionProps<MentionPerson>) => {
      props = p;
      index = Math.min(index, Math.max(0, p.items.length - 1));
      draw();
    },
    onKeyDown: ({ event }: SuggestionKeyDownProps) => {
      if (!props || !props.items.length) return false;
      if (event.key === "ArrowDown") {
        index = (index + 1) % props.items.length;
        draw();
        return true;
      }
      if (event.key === "ArrowUp") {
        index = (index - 1 + props.items.length) % props.items.length;
        draw();
        return true;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        const p = props.items[index]!;
        props.command({ id: p.id, label: p.label });
        return true;
      }
      if (event.key === "Escape") {
        el?.remove();
        el = null;
        return true;
      }
      return false;
    },
    onExit: () => {
      el?.remove();
      el = null;
    },
  };
}

export function Toolbar({ editor }: { editor: TiptapEditor }) {
  const b = (label: string, title: string, active: boolean, run: () => void) => (
    <button type="button" className={"tb" + (active ? " on" : "")} title={title} aria-label={title} aria-pressed={active} onMouseDown={(e) => e.preventDefault()} onClick={run}>
      {label}
    </button>
  );
  const c = () => editor.chain().focus();
  return (
    <div className="editor-toolbar" role="toolbar">
      {b("B", "Bold", editor.isActive("bold"), () => c().toggleBold().run())}
      {b("I", "Italic", editor.isActive("italic"), () => c().toggleItalic().run())}
      {b("S", "Strikethrough", editor.isActive("strike"), () => c().toggleStrike().run())}
      {b("</>", "Code", editor.isActive("code"), () => c().toggleCode().run())}
      <span className="tb-sep" />
      {b("H2", "Heading", editor.isActive("heading", { level: 2 }), () => c().toggleHeading({ level: 2 }).run())}
      {b("•", "Bullet list", editor.isActive("bulletList"), () => c().toggleBulletList().run())}
      {b("1.", "Numbered list", editor.isActive("orderedList"), () => c().toggleOrderedList().run())}
      {b("☑", "Task list", editor.isActive("taskList"), () => c().toggleTaskList().run())}
      {b("❝", "Quote", editor.isActive("blockquote"), () => c().toggleBlockquote().run())}
      {b("{ }", "Code block", editor.isActive("codeBlock"), () => c().toggleCodeBlock().run())}
    </div>
  );
}
