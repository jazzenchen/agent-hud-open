import { createContext, useContext, useMemo, type ReactNode } from "react";
import { statusColor } from "./model";
import {
  lookGround,
  markLevelText,
  type Ground,
  type MarkLevel,
} from "./tokens";

const LookGround = createContext(false);

/** Tells what the HUD draws inside whether it lies on a ground the desktop
 * shows through (Open's `hudLook` in the environment): a solid surface with
 * a transparency. Set once where a screen's look is drawn, the HUD itself and
 * every picture of it. */
export function HUDGroundProvider({
  look,
  children,
}: {
  look: { solidTransparency: number; hudMaterial?: string };
  children: ReactNode;
}) {
  const { seeThrough = false } = lookGround(look, false);
  return (
    <LookGround.Provider value={seeThrough}>{children}</LookGround.Provider>
  );
}

/** The ground a piece of the HUD drawn `light` or dark stands on: its own
 * appearance, which can differ from the look's (the notch row stays dark),
 * on the look's see-through or opaque ground. */
export function useGround(light: boolean): Ground {
  const seeThrough = useContext(LookGround);
  return useMemo(() => ({ light, seeThrough }), [light, seeThrough]);
}

/** A mark's level as text on the ground of a piece drawn `light` or dark: the
 * mark's own colour on an opaque ground, as before, and the level's
 * see-through text shade (the idle grey's for none) where the desktop shows
 * through, where the bright fills fall to about 1.5:1 on a mid-grey ground. */
export function useMarkLevelText(light: boolean): (level: MarkLevel) => string {
  const ground = useGround(light);
  return useMemo(
    () =>
      ground.seeThrough
        ? (level: MarkLevel) => markLevelText(level, ground)
        : statusColor,
    [ground],
  );
}
