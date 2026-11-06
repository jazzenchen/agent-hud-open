import type { Theme } from "./SessionTheme";

/** Turn heights in the drawing: context grows turn by turn, drops once where
 * it was compacted, and grows again. */
const TURNS = [16, 22, 20, 30, 27, 38, 34, 46, 18, 24, 31, 40, 36, 49, 45, 56];
const COMPACTED = 8;
/** The sub-agents' lanes: where each starts and ends, as shares of the run. */
const LANES = [
  [0.08, 0.42],
  [0.3, 0.74],
  [0.58, 0.95],
] as const;

/** A session sketched faintly where the detail column waits for one: a card
 * with its turns as bars of tokens, the context line above them with its
 * compaction, and its sub-agents' lanes. It draws itself in once. */
export function NoSessionArt({ theme }: { theme: Theme }) {
  const left = 42;
  const base = 128;
  const step = 13.75;
  const bar = 9;
  const tops = TURNS.map((height) => base - height);
  const line = TURNS.map((height, index) => {
    const x = left + index * step + bar / 2;
    return `${index ? "L" : "M"}${x} ${base - height - 7}`;
  }).join(" ");
  const lanes = ["reasoning", "cacheWrite", "input"] as const;
  return (
    <svg
      viewBox="0 0 300 190"
      style={{ width: "min(300px, 80%)", height: "auto", display: "block" }}
      aria-hidden
    >
      <rect
        x={42}
        y={6}
        width={232}
        height={156}
        rx={12}
        fill={theme.card}
        stroke={theme.border}
        opacity={0.7}
      />
      <g className="session-art-card">
        <rect
          x={24}
          y={20}
          width={252}
          height={160}
          rx={12}
          fill={theme.surface}
          stroke={theme.border}
        />
        <circle cx={44} cy={40} r={6} fill={theme.accent} opacity={0.75} />
        <rect x={56} y={35} width={74} height={7} rx={3.5} fill={theme.track} />
        <rect
          x={56}
          y={46}
          width={44}
          height={5}
          rx={2.5}
          fill={theme.track}
          opacity={0.6}
        />
        <rect
          x={222}
          y={35}
          width={38}
          height={10}
          rx={5}
          fill={theme.track}
          opacity={0.7}
        />
        <line
          x1={left + COMPACTED * step - 2.4}
          x2={left + COMPACTED * step - 2.4}
          y1={62}
          y2={base}
          stroke={theme.compaction}
          strokeWidth={1}
          strokeDasharray="2 3"
          opacity={0.6}
        />
        {TURNS.map((height, index) => {
          const x = left + index * step;
          const cache = height * 0.58;
          const input = height * 0.27;
          const output = height - cache - input;
          return (
            <g
              key={index}
              className="session-art-bar"
              style={{ animationDelay: `${index * 32}ms` }}
            >
              <rect
                x={x}
                y={base - cache}
                width={bar}
                height={cache}
                rx={1.5}
                fill={theme.cache}
                opacity={0.5}
              />
              <rect
                x={x}
                y={base - cache - input}
                width={bar}
                height={input - 1}
                rx={1.5}
                fill={theme.input}
                opacity={0.55}
              />
              <rect
                x={x}
                y={tops[index]}
                width={bar}
                height={output - 1}
                rx={1.5}
                fill={theme.output}
                opacity={0.6}
              />
            </g>
          );
        })}
        <path
          className="session-art-line"
          d={line}
          pathLength={1}
          fill="none"
          stroke={theme.secondary}
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeLinejoin="round"
          opacity={0.55}
        />
        <line
          x1={38}
          x2={262}
          y1={base + 0.5}
          y2={base + 0.5}
          stroke={theme.divider}
        />
        {LANES.map(([from, to], index) => {
          const y = 143 + index * 11;
          const x = left + from * (step * TURNS.length);
          const width = (to - from) * step * TURNS.length;
          return (
            <g
              key={index}
              className="session-art-lane"
              style={{ animationDelay: `${380 + index * 90}ms` }}
            >
              <circle
                cx={x}
                cy={y}
                r={3}
                fill={theme[lanes[index]]}
                opacity={0.75}
              />
              <rect
                x={x + 6}
                y={y - 2}
                width={width - 6}
                height={4}
                rx={2}
                fill={theme[lanes[index]]}
                opacity={0.4}
              />
            </g>
          );
        })}
      </g>
    </svg>
  );
}
