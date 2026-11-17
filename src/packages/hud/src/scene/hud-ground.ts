/** The HUD's ground: its fill, and the outline and shadow that belong to it.
 * Transparency runs it from opaque (`alpha` 1) to fully clear, the outline
 * and shadow fading with it; logos, bots and text are drawn over it at full
 * strength. */
export function hudGround(light: boolean, alpha: number) {
  return {
    fill: light ? `rgba(245,245,247,${alpha})` : `rgba(0,0,0,${alpha})`,
    outline: light
      ? `rgba(0,0,0,${0.106 * alpha})`
      : `rgba(255,255,255,${0.075 * alpha})`,
    shadow: `0 9px 30px rgba(0,0,0,${0.133 * alpha})`,
  };
}
