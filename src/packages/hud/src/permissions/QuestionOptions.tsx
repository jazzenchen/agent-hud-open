import { useT } from "@agent-hud/ui/format-context";
import type { RefObject } from "react";
import { Check } from "lucide-react";
import type { ApprovalQuestion } from "@agent-hud/protocol";
import { alertFill, onAlertFill } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import type { Draft } from "./PermissionQuestionCard";

/** A question's offered answers, to pick one or several, and the field for
 * an answer in one's own words. */
export function QuestionOptions({
  question: q,
  value,
  hasCustom,
  editingCustom,
  disabled,
  light,
  customInputRef,
  changeAnswer,
  setEditingCustomKey,
  advanceOrSubmit,
}: {
  question: ApprovalQuestion;
  value: Draft;
  hasCustom: boolean;
  editingCustom: boolean;
  disabled: boolean;
  light: boolean;
  customInputRef: RefObject<HTMLInputElement | null>;
  changeAnswer: (key: string, patch: Partial<Draft>) => void;
  setEditingCustomKey: (key: string | null) => void;
  advanceOrSubmit: () => void;
}) {
  const t = useT();
  const key = q.answerKey;

  const ink = (alpha: number) =>
    light ? `rgba(0,0,0,${alpha})` : `rgba(255,255,255,${alpha})`;
  const primary = ink(0.92);
  const secondary = ink(0.55);
  const inset = light ? "rgba(0,0,0,0.045)" : "rgba(255,255,255,0.055)";
  const highlight = light ? "rgba(0,0,0,0.08)" : "rgba(255,255,255,0.1)";
  const ground = useGround(light);
  const signal = alertFill("warning", ground);
  const onSignal = onAlertFill(ground);

  const renderMarker = (chosen: boolean, multi: boolean) => (
    <span
      aria-hidden="true"
      style={{
        width: 12,
        height: 12,
        borderRadius: multi ? 3 : "50%",
        border: `1.2px solid ${chosen ? signal : ink(0.28)}`,
        background: chosen ? signal : "transparent",
        display: "inline-grid",
        placeItems: "center",
        flexShrink: 0,
        marginTop: 1,
      }}
    >
      {chosen && <Check size={7} strokeWidth={3} color={onSignal} />}
    </span>
  );

  return (
    <div style={cardColumn(4)}>
      {q.options.map((option) => {
        const chosen = value.selected.includes(option.label);
        return (
          <button
            key={option.label}
            type="button"
            disabled={disabled}
            onClick={() => {
              setEditingCustomKey(null);
              if (q.multiSelect) {
                const nextSelected = q.options
                  .map((o) => o.label)
                  .filter((label) =>
                    label === option.label
                      ? !chosen
                      : value.selected.includes(label),
                  );
                changeAnswer(key, { selected: nextSelected });
              } else {
                changeAnswer(key, { selected: [option.label], own: false });
              }
            }}
            style={{
              display: "flex",
              alignItems: "flex-start",
              gap: 7,
              width: "100%",
              padding: "6px 8px",
              borderRadius: 6,
              border: `1px solid ${chosen ? "rgba(233,161,109,0.45)" : "transparent"}`,
              background: chosen ? highlight : inset,
              color: "inherit",
              textAlign: "left",
              cursor: disabled ? "default" : "pointer",
            }}
          >
            {renderMarker(chosen, q.multiSelect)}
            <span
              style={{
                ...cardColumn(2),
                minWidth: 0,
                flex: 1,
              }}
            >
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 500,
                  color: primary,
                }}
              >
                {option.label}
              </span>
              {/* Decided 2026-10-21: shown whole, not cut at two lines (Open's lineLimit(2)). It says what picking
                this answer does, and the end of a long one is often the part that decides. */}
              {option.description && (
                <span
                  style={{
                    fontSize: 10,
                    lineHeight: 1.35,
                    color: secondary,
                    overflowWrap: "anywhere",
                  }}
                >
                  {option.description}
                </span>
              )}
            </span>
          </button>
        );
      })}
      {editingCustom ? (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            padding: "6px 8px",
            borderRadius: 6,
            border: "1px solid rgba(233,161,109,0.45)",
            background: hasCustom ? highlight : inset,
          }}
        >
          {renderMarker(hasCustom, q.multiSelect)}
          <input
            ref={customInputRef}
            type="text"
            aria-label={t("Your own answer")}
            placeholder={
              q.options.length === 0 ? t("Type an answer") : t("Other answer")
            }
            value={value.custom ?? ""}
            disabled={disabled}
            onChange={(e) => {
              const textVal = e.target.value;
              changeAnswer(key, {
                custom: textVal,
                own: true,
                ...(q.multiSelect ? {} : { selected: [] }),
              });
            }}
            onKeyDown={(e) => {
              // Return while an input method is composing belongs to it.
              if (
                e.key === "Enter" &&
                !e.nativeEvent.isComposing &&
                e.keyCode !== 229
              ) {
                e.preventDefault();
                advanceOrSubmit();
              }
            }}
            style={{
              flex: 1,
              border: 0,
              outline: 0,
              background: "transparent",
              color: primary,
              fontSize: 11,
              fontFamily: "inherit",
            }}
          />
        </div>
      ) : (
        <button
          type="button"
          disabled={disabled}
          onClick={() => {
            // Going into the field is choosing to answer in one's own words.
            changeAnswer(key, {
              own: true,
              ...(q.multiSelect ? {} : { selected: [] }),
            });
            setEditingCustomKey(key);
          }}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            width: "100%",
            padding: "6px 8px",
            borderRadius: 6,
            border: `1px solid ${hasCustom ? "rgba(233,161,109,0.45)" : "transparent"}`,
            background: hasCustom ? highlight : inset,
            color: "inherit",
            textAlign: "left",
            cursor: disabled ? "default" : "pointer",
          }}
        >
          {renderMarker(hasCustom, q.multiSelect)}
          <span
            style={{
              fontSize: 11,
              fontWeight: 500,
              color: hasCustom ? primary : secondary,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {value.custom?.trim() ? value.custom : t("Other")}
          </span>
        </button>
      )}
    </div>
  );
}
