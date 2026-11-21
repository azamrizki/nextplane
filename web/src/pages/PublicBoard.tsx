// A published board at /p/<token>: read without an account (SPRINT S3.5); with one, comment,
// react and vote where the project allows it (S3.7).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LogIn, SmilePlus, ThumbsDown, ThumbsUp, Trash2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router";
import { del, get, patch, post } from "../api/client";
import { Avatar } from "../components/Avatar";
import { Popover } from "../components/Layout";
import { errorText } from "../lib/errors";
import { useMe } from "../lib/session";
import { toast } from "../lib/toast";
import type { Priority, StateGroup } from "../api/types";
import { Dialog } from "../components/Dialog";
import { PriorityIcon, StateIcon, fmtDate } from "../components/Props";
import { FullPageSpinner } from "../components/ui";
import { useI18n } from "../i18n";

interface PState {
  id: string;
  name: string;
  color: string;
  group: StateGroup;
}
interface PLabel {
  id: string;
  name: string;
  color: string;
}
interface Tally {
  emoji: string;
  count: number;
  mine: boolean;
}
interface PComment {
  id: string;
  author_name: string;
  author_avatar: string | null;
  body_html: string;
  created_at: string;
  edited: boolean;
  mine: boolean;
  reactions: Tally[];
}
interface PItem {
  sequence: number;
  /** Present when the board allows them. */
  reactions?: Tally[];
  votes?: { up: number; down: number; mine: number };
  comments?: PComment[];
  title: string;
  state_id: string;
  priority: Priority;
  start_date: string | null;
  due_date: string | null;
  estimate: string | null;
  label_ids: string[];
  description_html?: string;
}
interface Board {
  workspace: string;
  project: { name: string; identifier: string };
  states: PState[];
  labels: PLabel[];
  items: PItem[];
}

export function PublicBoardPage() {
  const { t, lang } = useI18n();
  const { token = "" } = useParams();
  const q = useQuery({ queryKey: ["public-board", token], queryFn: () => get<Board>(`/public/boards/${token}`) });
  const [layout, setLayout] = useState<"list" | "board">("board");
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState<number | null>(null);
  const b = q.data;
  const shown = useMemo(() => {
    const s = search.trim().toLowerCase();
    return (b?.items ?? []).filter((i) => !s || i.title.toLowerCase().includes(s) || `${b?.project.identifier}-${i.sequence}`.toLowerCase() === s);
  }, [b, search]);
  if (q.isLoading) return <FullPageSpinner />;
  if (!b) return <div className="pb-gone"><h1>{t("pb.gone")}</h1></div>;
  const key = (i: PItem) => `${b.project.identifier}-${i.sequence}`;
  const labelsOf = (i: PItem) => b.labels.filter((l) => i.label_ids.includes(l.id));
  const card = (i: PItem) => (
    <button key={i.sequence} className="pb-card" onClick={() => setOpen(i.sequence)}>
      <span className="row-wrap">
        <PriorityIcon p={i.priority} />
        <span className="mono faint small">{key(i)}</span>
      </span>
      <span className="pb-title">{i.title}</span>
      <span className="row-wrap">
        {labelsOf(i).map((l) => (
          <span key={l.id} className="label-chip small">
            <span className="chart-swatch" style={{ background: l.color, borderRadius: "50%" }} />
            {l.name}
          </span>
        ))}
        {i.due_date ? <span className="faint small">{fmtDate(i.due_date, lang)}</span> : null}
      </span>
    </button>
  );
  return (
    <div className="pb-page">
      <header className="pb-head">
        <div>
          <h1>{b.project.name}</h1>
          <span className="faint small">{b.workspace} · {t("pb.readOnly")}</span>
        </div>
        <div className="row-wrap">
          <input className="input" type="search" placeholder={t("pb.search")} aria-label={t("pb.search")} value={search} onChange={(e) => setSearch(e.target.value)} />
          <div className="seg" role="tablist">
            {(["board", "list"] as const).map((l) => (
              <button key={l} role="tab" aria-selected={layout === l} className={layout === l ? "on" : ""} onClick={() => setLayout(l)}>{t(`pb.${l}`)}</button>
            ))}
          </div>
        </div>
      </header>
      {!shown.length ? <p className="faint">{t("pb.empty")}</p> : null}
      {layout === "board" ? (
        <div className="pb-board">
          {b.states.map((s) => {
            const col = shown.filter((i) => i.state_id === s.id);
            return (
              <section key={s.id} className="pb-col" aria-label={s.name}>
                <h2 className="pb-col-head">
                  <StateIcon group={s.group} color={s.color} />
                  {s.name}
                  <span className="faint">{col.length}</span>
                </h2>
                {col.map(card)}
              </section>
            );
          })}
        </div>
      ) : (
        <div className="pb-list">
          {b.states.map((s) => {
            const rows = shown.filter((i) => i.state_id === s.id);
            if (!rows.length) return null;
            return (
              <section key={s.id}>
                <h2 className="pb-col-head">
                  <StateIcon group={s.group} color={s.color} />
                  {s.name}
                  <span className="faint">{rows.length}</span>
                </h2>
                {rows.map(card)}
              </section>
            );
          })}
        </div>
      )}
      {open !== null ? <PublicItemDialog token={token} board={b} seq={open} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}

function PublicItemDialog({ token, board, seq, onClose }: { token: string; board: Board; seq: number; onClose: () => void }) {
  const { lang } = useI18n();
  const q = useQuery({ queryKey: ["public-board", token, seq], queryFn: () => get<PItem>(`/public/boards/${token}/items/${seq}`) });
  const i = q.data;
  const st = board.states.find((s) => s.id === i?.state_id);
  return (
    <Dialog open onClose={onClose} title={i ? `${board.project.identifier}-${i.sequence} ${i.title}` : "…"} width={680}>
      {i ? (
        <div className="form">
          <div className="row-wrap small">
            {st ? (
              <span className="row-wrap">
                <StateIcon group={st.group} color={st.color} />
                {st.name}
              </span>
            ) : null}
            <PriorityIcon p={i.priority} />
            {i.due_date ? <span className="faint">{fmtDate(i.due_date, lang)}</span> : null}
          </div>
          {/* Stored HTML is sanitized on the server. */}
          {i.description_html ? <div className="prose" dangerouslySetInnerHTML={{ __html: i.description_html }} /> : null}
          <Interactions token={token} item={i} />
        </div>
      ) : (
        <FullPageSpinner />
      )}
    </Dialog>
  );
}

const EMOJI = ["👍", "❤️", "🎉", "👀", "🚀", "😄"];
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Reactions, votes and comments a board allows (S3.7): anyone reads them; signing in lets you
 * add yours. */
function Interactions({ token, item }: { token: string; item: PItem }) {
  const { t, lang } = useI18n();
  const qc = useQueryClient();
  const me = useMe();
  const navigate = useNavigate();
  const location = useLocation();
  const [text, setText] = useState("");
  const url = `/public/boards/${token}/items/${item.sequence}`;
  const signedIn = !!me.data;
  const signIn = () => navigate("/login", { state: { from: location.pathname + location.search } });
  const act = useMutation({
    mutationFn: (v: { method: "post" | "patch" | "del"; path: string; body?: unknown }) =>
      v.method === "del" ? del<PItem>(url + v.path) : v.method === "patch" ? patch<PItem>(url + v.path, v.body) : post<PItem>(url + v.path, v.body),
    onSuccess: (next) => qc.setQueryData(["public-board", token, item.sequence], next),
    onError: (e) => toast({ kind: "error", title: errorText(e, t) }),
  });
  const react = (emoji: string, comment_id?: string) => (signedIn ? act.mutate({ method: "post", path: "/reactions", body: { emoji, comment_id } }) : signIn());
  const vote = (v: number) => (signedIn ? act.mutate({ method: "post", path: "/votes", body: { value: item.votes?.mine === v ? 0 : v } }) : signIn());
  if (!item.reactions && !item.votes && !item.comments) return null;
  return (
    <div className="board-interactions">
      <div className="row-wrap">
        {item.votes ? (
          <span className="vote-group">
            <button className={"btn btn-sm" + (item.votes.mine === 1 ? " on" : "")} aria-pressed={item.votes.mine === 1} onClick={() => vote(1)} aria-label={t("pb.up")}>
              <ThumbsUp /> {item.votes.up}
            </button>
            <button className={"btn btn-sm" + (item.votes.mine === -1 ? " on" : "")} aria-pressed={item.votes.mine === -1} onClick={() => vote(-1)} aria-label={t("pb.down")}>
              <ThumbsDown /> {item.votes.down}
            </button>
          </span>
        ) : null}
        {item.reactions ? <Reacts list={item.reactions} onReact={(e) => react(e)} /> : null}
      </div>
      {item.comments ? (
        <section className="board-comments" aria-label={t("pb.comments")}>
          <h3>{t("pb.comments")}</h3>
          {signedIn ? (
            <div className="board-composer">
              <textarea className="textarea" placeholder={t("pb.placeholder")} aria-label={t("pb.placeholder")} value={text} onChange={(e) => setText(e.target.value)} rows={2} />
              <button
                className="btn btn-sm btn-primary"
                disabled={!text.trim() || act.isPending}
                onClick={() =>
                  act.mutate(
                    { method: "post", path: "/comments", body: { body_html: text.trim().split(/\n{2,}/).map((p) => `<p>${esc(p).replace(/\n/g, "<br>")}</p>`).join("") } },
                    { onSuccess: () => setText("") },
                  )
                }
              >
                {t("pb.add")}
              </button>
            </div>
          ) : (
            <button className="btn btn-sm" onClick={signIn}>
              <LogIn />
              {t("pb.signIn")}
            </button>
          )}
          {!item.comments.length ? <p className="muted small">{t("pb.noComments")}</p> : null}
          <ol className="comments">
            {item.comments.map((c) => (
              <li key={c.id} className="comment">
                <Avatar name={c.author_name} src={c.author_avatar} size={24} />
                <div className="comment-body">
                  <div className="comment-head">
                    <b>{c.author_name}</b>
                    <span className="faint small">{new Date(c.created_at).toLocaleString(lang === "id" ? "id-ID" : "en-US", { dateStyle: "medium", timeStyle: "short" })}</span>
                    {c.edited ? <span className="faint small">({t("pb.edited")})</span> : null}
                    <span className="grow" />
                    {c.mine ? (
                      <button className="icon-btn icon-btn-sm" aria-label={t("pb.delete")} title={t("pb.delete")} onClick={() => act.mutate({ method: "del", path: `/comments/${c.id}` })}>
                        <Trash2 />
                      </button>
                    ) : null}
                  </div>
                  {/* Stored HTML is sanitized on the server. */}
                  <div className="prose comment-html" dangerouslySetInnerHTML={{ __html: c.body_html }} />
                  {item.reactions ? <Reacts list={c.reactions} onReact={(e) => react(e, c.id)} /> : null}
                </div>
              </li>
            ))}
          </ol>
        </section>
      ) : null}
    </div>
  );
}

function Reacts({ list, onReact }: { list: Tally[]; onReact: (emoji: string) => void }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  return (
    <span className="reactions">
      {list.map((r) => (
        <button key={r.emoji} className={"reaction" + (r.mine ? " mine" : "")} aria-pressed={r.mine} onClick={() => onReact(r.emoji)}>
          {r.emoji} {r.count}
        </button>
      ))}
      <Popover
        open={open}
        onClose={close}
        trigger={
          <button className="reaction" onClick={() => setOpen(!open)} aria-label={t("pb.react")} title={t("pb.react")}>
            <SmilePlus />
          </button>
        }
      >
        <div className="menu emoji-menu" role="menu">
          {EMOJI.map((e) => (
            <button key={e} className="menu-item" role="menuitem" onClick={() => (close(), onReact(e))}>
              {e}
            </button>
          ))}
        </div>
      </Popover>
    </span>
  );
}
