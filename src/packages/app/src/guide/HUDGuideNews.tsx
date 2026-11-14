// A release's news in the guide's window beside the HUD, with the Star last.
import { Star, X } from "lucide-react";
import type { HUDGuideCardProps } from "./HUDGuideCard";
import { Button } from "@agent-hud/ui/components/button";
import { Actions, Header } from "./hud-guide-parts";
import { unspaced, type Translate } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { guideText } from "./copy";

export function News({ view, t, close, tour, show, star }: HUDGuideCardProps) {
  const { language } = useFormat();
  if (view.kind !== "news") return null;
  return (
    <>
      <Header
        label={t("New in {{version}}", { version: view.version })}
        close={close}
        t={t}
      />
      {view.items.map((item) => (
        <div
          key={typeof item.title === "string" ? item.title : item.title[0]}
          style={{
            display: "flex",
            gap: 10,
            alignItems: "flex-start",
            marginTop: 8,
          }}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 13.5, fontWeight: 600 }}>
              {guideText(t, language, item.title)}
            </div>
            {item.body && (
              <div style={{ opacity: 0.72 }}>
                {guideText(t, language, item.body)}
              </div>
            )}
          </div>
          {item.target && (
            <Button
              size="xs"
              variant="ghost"
              className="text-primary"
              onClick={() => show(item.target!)}
            >
              {t("Show me")}
            </Button>
          )}
        </div>
      ))}
      {view.star && <StarLine t={t} star={star} />}
      <Actions>
        <Button size="sm" variant="ghost" onClick={tour}>
          {t("Take me around")}
        </Button>
        <Button size="sm" onClick={close}>
          {t("Got it")}
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
        marginTop: 10,
        paddingTop: 9,
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
