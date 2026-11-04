import { useEffect, useRef, useState } from "react";
import { ArrowRight, Check, ChevronLeft, LoaderCircle } from "lucide-react";
import type {
  Approval,
  RequestAnswer,
  RequestDecision,
} from "@agent-hud/protocol";
import { alertText, permissionColors } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { CardButton } from "./CardButton";
import { QuestionAnswerElsewhere } from "./QuestionAnswerElsewhere";
import { QuestionOptions } from "./QuestionOptions";

/** A question's draft: the offered answers picked, the words typed, and
 * whether those words are the answer (`own`). Skipping or picking a single
 * option sets the words aside without erasing them, as Open's
 * QuestionDraft does. */
export type Draft = RequestAnswer & { own?: boolean };

const drafts = new Map<
  string,
  { answers: Record<string, Draft>; page: number }
>();

export function PermissionQuestionCard({
  request,
  disabled,
  pending,
  light = false,
  zh = false,
  answer,
  openSettings,
}: {
  request: Approval;
  disabled: boolean;
  pending: boolean;
  light?: boolean;
  zh?: boolean;
  answer: (decision: RequestDecision) => void | Promise<void>;
  /** Settings, on a page such as "agent-plugins"; without it the card only says where to go. */
  openSettings?: (page: string) => void;
}) {
  const t = (en: string, cn: string) => (zh ? cn : en);
  const [answers, setAnswers] = useState<Record<string, Draft>>(
    () => drafts.get(request.id)?.answers ?? {},
  );
  const [page, setPageState] = useState(
    () => drafts.get(request.id)?.page ?? 0,
  );
  const [editingCustomKey, setEditingCustomKey] = useState<string | null>(null);
  const customInputRef = useRef<HTMLInputElement | null>(null);

  const setPage = (nextPage: number) => {
    setPageState(nextPage);
    const current = drafts.get(request.id);
    drafts.set(request.id, {
      answers: current?.answers ?? answers,
      page: nextPage,
    });
  };

  const questions = request.questions ?? [];
  const canAnswerQuestion =
    request.answersQuestion !== false && questions.length > 0;
  const detailText = request.detail?.trim() || null;

  const ink = (alpha: number) =>
    light ? `rgba(0,0,0,${alpha})` : `rgba(255,255,255,${alpha})`;
  const primary = ink(0.92);
  const secondary = ink(0.55);
  const tertiary = ink(0.38);
  const ground = useGround(light);
  const controls = permissionColors(ground);
  const allow = controls.allowFill;
  const onAllow = controls.onAllow;
  const signalText = alertText("warning", ground);

  const changeAnswer = (key: string, patch: Partial<Draft>) =>
    setAnswers((previous) => {
      const next = {
        ...previous,
        [key]: {
          ...previous[key],
          ...patch,
          selected: patch.selected ?? previous[key]?.selected ?? [],
        },
      };
      drafts.set(request.id, {
        answers: next,
        page: drafts.get(request.id)?.page ?? page,
      });
      return next;
    });

  const submitAllAnswers = (omitKey?: string) => {
    // Several picks are filed in the order they were offered; the typed
    // words only when they are the chosen answer.
    const written = Object.fromEntries(
      Object.entries(answers)
        .map(([key, value]) => {
          const offered =
            questions
              .find((q) => q.answerKey === key)
              ?.options.map((o) => o.label) ?? value.selected;
          const selected = offered.filter((label) =>
            value.selected.includes(label),
          );
          const custom = value.own ? value.custom?.trim() : undefined;
          return [key, { selected, ...(custom ? { custom } : {}) }] as const;
        })
        .filter(
          ([key, value]) =>
            key !== omitKey && (value.selected.length > 0 || "custom" in value),
        ),
    );
    drafts.delete(request.id);
    void answer({ kind: "answer", answers: written });
  };

  const safePage = Math.min(page, Math.max(0, questions.length - 1));
  // None when the card cannot answer and was sent no questions.
  const q = questions[safePage];
  const key = q?.answerKey;
  const editingCustom = editingCustomKey === key || q?.options.length === 0;
  useEffect(() => {
    if (!editingCustom) return;
    const input = customInputRef.current;
    if (!input || input.closest("[inert], [aria-hidden=true]")) return;
    input.focus({ preventScroll: true });
  }, [editingCustom, key]);

  if (!canAnswerQuestion)
    return (
      <QuestionAnswerElsewhere
        request={request}
        disabled={disabled}
        light={light}
        zh={zh}
        openSettings={openSettings}
        dismiss={() => {
          drafts.delete(request.id);
          void answer({ kind: "leave" });
        }}
      />
    );

  const value = answers[key] ?? { selected: [] };
  const hasCustom = Boolean(value.own && value.custom?.trim());
  const isAnswered = value.selected.length > 0 || hasCustom;
  const advanceOrSubmit = () => {
    if (!isAnswered || disabled) return;
    setEditingCustomKey(null);
    if (safePage + 1 < questions.length) {
      setPage(safePage + 1);
    } else {
      submitAllAnswers();
    }
  };

  return (
    <div style={cardColumn(8)}>
      {detailText && (
        <div style={{ fontSize: 11, color: secondary, lineHeight: 1.35 }}>
          {detailText}
        </div>
      )}
      {(q.header || questions.length > 1) && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
          }}
        >
          {q.header && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 700,
                padding: "2px 6px",
                borderRadius: 4,
                background: "rgba(233, 161, 109, 0.14)",
                color: signalText,
              }}
            >
              {q.header}
            </span>
          )}
          <span style={{ flex: 1 }} />
          {questions.length > 1 && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 500,
                fontVariantNumeric: "tabular-nums",
                color: tertiary,
              }}
            >
              {safePage + 1} / {questions.length}
            </span>
          )}
        </div>
      )}
      <div style={cardColumn(2)}>
        <div
          style={{
            fontSize: 12,
            fontWeight: 600,
            color: primary,
            lineHeight: 1.4,
          }}
        >
          {q.question}
        </div>
        {q.multiSelect && (
          <div style={{ fontSize: 10, color: tertiary }}>
            {t("Choose any", "可多选")}
          </div>
        )}
      </div>
      <QuestionOptions
        question={q}
        value={value}
        hasCustom={hasCustom}
        editingCustom={editingCustom}
        disabled={disabled}
        light={light}
        zh={zh}
        customInputRef={customInputRef}
        changeAnswer={changeAnswer}
        setEditingCustomKey={setEditingCustomKey}
        advanceOrSubmit={advanceOrSubmit}
      />
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <CardButton
          disabled={disabled}
          onClick={() => {
            setEditingCustomKey(null);
            changeAnswer(key, { selected: [], own: false });
            if (safePage + 1 < questions.length) {
              setPage(safePage + 1);
            } else {
              submitAllAnswers(key);
            }
          }}
          edge={ink(0.18)}
          color={secondary}
        >
          <ArrowRight size={9} strokeWidth={2.6} />
          {t("Skip", "跳过")}
        </CardButton>
        <span style={{ flex: 1, minWidth: 8 }} />
        {safePage > 0 && (
          <CardButton
            disabled={disabled}
            onClick={() => {
              setEditingCustomKey(null);
              setPage(safePage - 1);
            }}
            edge={ink(0.18)}
            color={secondary}
          >
            <ChevronLeft size={9} strokeWidth={2.6} />
            {t("Back", "返回")}
          </CardButton>
        )}
        <CardButton
          disabled={disabled || !isAnswered}
          onClick={advanceOrSubmit}
          fill={allow}
          color={onAllow}
          style={{ opacity: isAnswered ? 1 : 0.45 }}
        >
          {pending ? (
            <LoaderCircle size={9} className="animate-spin" />
          ) : safePage + 1 < questions.length ? (
            <ArrowRight size={9} strokeWidth={2.6} />
          ) : (
            <Check size={9} strokeWidth={2.6} />
          )}
          {safePage + 1 < questions.length
            ? t("Next", "继续")
            : t("Submit", "提交")}
        </CardButton>
      </div>
    </div>
  );
}
