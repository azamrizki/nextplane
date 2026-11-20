// A published, read-only board at /p/<token>: no account needed (SPRINT S3.5).
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { useParams } from "react-router";
import { get } from "../api/client";
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
interface PItem {
  sequence: number;
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
        </div>
      ) : (
        <FullPageSpinner />
      )}
    </Dialog>
  );
}
