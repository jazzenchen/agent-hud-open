import type { ReactNode } from "react";
import { Plug, X } from "lucide-react";
import type { Approval } from "@agent-hud/protocol";
import { clientDisplayName } from "../IslandNews";
import { alertText, permissionColors } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { CardButton } from "./CardButton";

/** Settings' page for what Agent HUD installs into each client. */
const AGENT_PLUGINS_PAGE = "agent-plugins";
/** What loads a newly installed plugin in a running Claude Code session. */
const RELOAD_COMMAND = "/reload-plugins";

/** `text` with each `command` in it drawn by `code`. */
function withCommand(
  text: string,
  command: string,
  code: (command: string, key: number) => ReactNode,
) {
  const parts = text.split(command);
  return parts.flatMap((part, index) =>
    index === 0 ? [part] : [code(command, index), part],
  );
}

/** A question this card cannot answer: only the question and why it waits
 * for the client. Offered answers that could not be sent would look like
 * buttons that do nothing. */
export function QuestionAnswerElsewhere({
  request,
  disabled,
  light,
  zh,
  openSettings,
  dismiss,
}: {
  request: Approval;
  disabled: boolean;
  light: boolean;
  zh: boolean;
  /** Settings, on a page such as "agent-plugins"; without it the card only says where to go. */
  openSettings?: (page: string) => void;
  dismiss: () => void;
}) {
  const t = (en: string, cn: string) => (zh ? cn : en);
  const questions = request.questions ?? [];
  const vendor = request.vendor ?? clientDisplayName(request.source);
  const detailText = request.detail?.trim() || null;

  const ink = (alpha: number) =>
    light ? `rgba(0,0,0,${alpha})` : `rgba(255,255,255,${alpha})`;
  const primary = ink(0.92);
  const secondary = ink(0.55);
  const inset = light ? "rgba(0,0,0,0.045)" : "rgba(255,255,255,0.055)";
  const highlight = light ? "rgba(0,0,0,0.08)" : "rgba(255,255,255,0.1)";
  const ground = useGround(light);
  const controls = permissionColors(ground);
  const allow = controls.allowFill;
  const onAllow = controls.onAllow;
  const signalText = alertText("warning", ground);
  const code = (command: string, key: number) => (
    <code
      key={key}
      style={{
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: 10,
        padding: "0 4px",
        borderRadius: 4,
        background: highlight,
        color: primary,
      }}
    >
      {command}
    </code>
  );

  const fix = request.answerFix ?? null;
  const opensPlugins =
    Boolean(openSettings) &&
    (fix === "installPlugin" || fix === "turnOnAnswers");
  return (
    <div style={cardColumn(8)}>
      {detailText && (
        <div style={{ fontSize: 11, color: secondary, lineHeight: 1.35 }}>
          {detailText}
        </div>
      )}
      {questions.map((q) => (
        <div key={q.answerKey} style={cardColumn(5)}>
          {q.header && (
            <span
              style={{
                justifySelf: "start",
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
        </div>
      ))}
      {/* Core says why this one can be answered only in the client and
          which step would let Agent HUD answer it: a plugin to install or
          answering to turn back on opens Settings → Agent plugins; a
          session that has not loaded the plugin is told the command. */}
      <div
        style={{
          ...cardColumn(3),
          padding: "7px 8px",
          borderRadius: 6,
          background: inset,
        }}
      >
        <span style={{ fontSize: 11, fontWeight: 600, color: primary }}>
          {t(`Answer in ${vendor}`, `请到 ${vendor} 里回答`)}
        </span>
        {request.answerNote && (
          <span
            style={{
              fontSize: 10,
              lineHeight: 1.4,
              color: secondary,
              overflowWrap: "anywhere",
            }}
          >
            {fix === "reloadPlugins"
              ? withCommand(request.answerNote, RELOAD_COMMAND, code)
              : request.answerNote}
          </span>
        )}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        {opensPlugins && (
          <CardButton
            onClick={() => openSettings?.(AGENT_PLUGINS_PAGE)}
            fill={allow}
            color={onAllow}
          >
            <Plug size={9} strokeWidth={2.6} />
            {t("Open Agent plugins", "打开智能体插件")}
          </CardButton>
        )}
        <span style={{ flex: 1, minWidth: 8 }} />
        <CardButton
          disabled={disabled}
          onClick={dismiss}
          edge={ink(0.18)}
          color={secondary}
        >
          <X size={9} strokeWidth={2.6} />
          {t("Dismiss", "知道了")}
        </CardButton>
      </div>
    </div>
  );
}
