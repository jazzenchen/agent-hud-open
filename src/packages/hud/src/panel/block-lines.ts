/* How the panel's blocks share lines: a block's span asks for a half, a third
 * or a quarter of a line, and blocks next to each other that ask the same share
 * it where each keeps enough room. */

const SHARES: Record<string, number> = { half: 2, third: 3, quarter: 4 };

/** How many blocks of `span` share a line `width` wide with `gap` between them:
 * as many as asked, fewer where each would get less than its least. A half
 * needs 230 points, as it always has; a third or a quarter, drawn compact,
 * 200. */
export function perLine(
  span: string | undefined,
  width: number,
  gap: number,
): number {
  const asked = SHARES[span ?? ""] ?? 1;
  const least = asked === 2 ? 230 : 200;
  for (let count = asked; count > 1; count--)
    if ((width - (count - 1) * gap) / count >= least) return count;
  return 1;
}

/** The blocks in lines, each line with the columns its blocks share: a run of
 * blocks asking the same share fills lines of `perLine`; what is left of a run
 * keeps the run's columns, except a half alone, which takes the line. */
export function blockLines<T extends { key: string }>(
  blocks: T[],
  span: (key: string) => string | undefined,
  width: number,
  gap: number,
): { blocks: T[]; columns: number }[] {
  const lines: { blocks: T[]; columns: number }[] = [];
  for (let i = 0; i < blocks.length;) {
    const asked = span(blocks[i].key);
    const count = perLine(asked, width, gap);
    const line = [blocks[i]];
    while (
      line.length < count &&
      i + line.length < blocks.length &&
      span(blocks[i + line.length].key) === asked
    )
      line.push(blocks[i + line.length]);
    const columns =
      count === 2 && line.length === 1 ? 1 : Math.max(line.length, count);
    lines.push({ blocks: line, columns });
    i += line.length;
  }
  return lines;
}
