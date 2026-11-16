// A release's news in the guide's window beside the HUD, with the Star last.
// Wider than a step of the tour: a few short paragraphs read as lines, not
// as a column.
import { Star, X } from "lucide-react";
import type { HUDGuideCardProps } from "./HUDGuideCard";
import { Button } from "@agent-hud/ui/components/button";
import { Actions, Header } from "./hud-guide-parts";
import { unspaced, type Translate } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { guideText } from "./copy";

/** The guide window's width while it holds the news. */
export const newsWidth = 360;

export function News({ view, t, close, tour, show, star }: HUDGuideCardProps) {
  const { language } = useFormat();
  if (view.kind !== "news") return null;
  return (
    <>
      <Header
        label={t("New in {{version}}", { version: view.version })}
        close={close}
        t={t}
        heading
      />
      {view.items.map((item) => (
        <div
          key={typeof item.title === "string" ? item.title : item.title[0]}
          style={{ marginTop: 10 }}
        >
          <div style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
            <div
              style={{ flex: 1, minWidth: 0, fontSize: 13, fontWeight: 600 }}
            >
              {guideText(t, language, item.title)}
            </div>
            {item.target && (
              <Button
                variant="link"
                onClick={() => show(item.target!)}
                className="h-auto shrink-0 p-0 text-xs"
              >
                {t("Show me")}
              </Button>
            )}
          </div>
          {item.body && (
            <div style={{ marginTop: 1, opacity: 0.72 }}>
              {guideText(t, language, item.body)}
            </div>
          )}
        </div>
      ))}
      {view.star && <StarLine t={t} star={star} />}
      <Actions>
        <Button size="sm" variant="secondary" onClick={close}>
          {t("Got it")}
        </Button>
        <Button size="sm" onClick={tour}>
          {t("Take me around")}
        </Button>
      </Actions>
    </>
  );
}

export function StarLine({
  t,
  star,
}: {
  t: Translate;
  star: (open: boolean) => void;
}) {
  const { language } = useFormat();
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 7,
        marginTop: 14,
        paddingTop: 10,
        borderTop: "1px solid rgba(128,128,128,0.25)",
        fontSize: 12,
      }}
    >
      <Star size={13} aria-hidden style={{ opacity: 0.6, flexShrink: 0 }} />
      <span style={{ flex: 1, opacity: 0.85 }}>
        {t("Like it?")}
        {unspaced(language) ? "" : " "}
        <Button
          variant="link"
          onClick={() => star(true)}
          className="h-auto p-0 text-xs"
        >
          {t("Star it on GitHub")}
        </Button>
      </span>
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={t("Don't ask again")}
        onClick={() => star(false)}
        className="opacity-60 hover:opacity-100"
      >
        <X />
      </Button>
    </div>
  );
}
