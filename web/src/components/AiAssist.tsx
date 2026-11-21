import { useMutation } from "@tanstack/react-query";
import { Sparkles, WandSparkles } from "lucide-react";
import { useCallback, useState } from "react";
import { ApiError, post } from "../api/client";
import type { InstanceInfoFull } from "../api/types";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { useInstance } from "../lib/session";
import { toast } from "../lib/toast";
import { Popover } from "./Layout";

interface Answer {
  response: string;
  response_html: string;
}

/** The AI writing assistant under a description (as in Plane's create modal): draft a
 * description from the title, or ask for anything and use the answer. Shown only when an
 * instance admin set it up. */
export function AiAssist({ slug, title, onUse }: { slug: string; title: string; onUse: (html: string) => void }) {
  const { t } = useI18n();
  const instance = useInstance();
  const [open, setOpen] = useState(false);
  const [task, setTask] = useState("");
  const close = useCallback(() => setOpen(false), []);
  const ask = useMutation({
    mutationFn: (body: { task: string; prompt?: string }) => post<Answer>(`/workspaces/${slug}/ai`, body),
    onError: (e) =>
      toast({ kind: "error", title: e instanceof ApiError && e.status === 429 ? t("ai.limited") : errorText(e, t) }),
  });
  const lucky = useMutation({
    mutationFn: () => post<Answer>(`/workspaces/${slug}/ai`, { task: t("ai.luckyTask"), prompt: title }),
    onSuccess: (a) => (a.response.trim() ? onUse(a.response_html) : toast({ kind: "info", title: t("ai.titleTooThin") })),
    onError: (e) =>
      toast({ kind: "error", title: e instanceof ApiError && e.status === 429 ? t("ai.limited") : errorText(e, t) }),
  });
  if (!(instance.data as InstanceInfoFull | undefined)?.ai_enabled) return null;
  const answer = ask.data;
  const generate = () => (task.trim() ? ask.mutate({ task, prompt: title }) : toast({ kind: "error", title: t("ai.emptyTask") }));
  return (
    <div className="ai-assist">
      {title.trim() ? (
        <button type="button" className="btn btn-sm btn-ghost" onClick={() => lucky.mutate()} disabled={lucky.isPending}>
          <WandSparkles />
          {lucky.isPending ? t("ai.writing") : t("ai.lucky")}
        </button>
      ) : null}
      <Popover
        open={open}
        onClose={close}
        trigger={
          <button type="button" className="btn btn-sm btn-ghost" onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="dialog">
            <Sparkles />
            {t("ai.ask")}
          </button>
        }
      >
        <div className="ai-pop" role="dialog" aria-label={t("ai.ask")}>
          {/* Not a <form>: this sits inside the create dialog's form. */}
          <div className="ai-form">
            <input
              className="input"
              autoFocus
              placeholder={t("ai.placeholder")}
              aria-label={t("ai.ask")}
              value={task}
              onChange={(e) => setTask(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  generate();
                }
              }}
            />
            <button type="button" className="btn btn-sm btn-primary" disabled={ask.isPending} onClick={generate}>
              {ask.isPending ? t("ai.writing") : answer ? t("ai.again") : t("ai.generate")}
            </button>
          </div>
          {answer ? (
            answer.response.trim() ? (
              <>
                <div className="ai-answer prose" dangerouslySetInnerHTML={{ __html: answer.response_html }} />
                <div className="ai-foot">
                  <span className="faint small">{t("ai.check")}</span>
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => {
                      onUse(answer.response_html);
                      close();
                    }}
                  >
                    {t("ai.use")}
                  </button>
                </div>
              </>
            ) : (
              <p className="muted small">{t("ai.empty")}</p>
            )
          ) : null}
        </div>
      </Popover>
    </div>
  );
}
