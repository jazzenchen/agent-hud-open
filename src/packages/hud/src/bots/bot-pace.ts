// BotMotion's frame grid and its seeded random source.
export const BotPace = {
  lively: 1 / 24,
  following: 1 / 12,
  calm: 1 / 6,
  slack: 0.004,
  next: (now: number, pace: number) =>
    (Math.floor((now + 0.004) / pace) + 1) * pace,
  frame(time: number, now: number) {
    return Math.max(
      this.next(now, this.lively),
      Math.ceil((time - this.slack) / this.lively) * this.lively,
    );
  },
  onFrame(now: number) {
    const frame = Math.round(now / this.lively) * this.lively;
    return Math.abs(frame - now) <= this.slack ? frame : now;
  },
};
export class SplitMix {
  state: bigint;
  constructor(
    seed: bigint | number = BigInt(
      Math.floor(Math.random() * Number.MAX_SAFE_INTEGER),
    ),
  ) {
    this.state = BigInt.asUintN(64, BigInt(seed));
  }
  next() {
    this.state = BigInt.asUintN(64, this.state + 0x9e3779b97f4a7c15n);
    let z = this.state;
    z = BigInt.asUintN(64, (z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
    z = BigInt.asUintN(64, (z ^ (z >> 27n)) * 0x94d049bb133111ebn);
    return z ^ (z >> 31n);
  }
  unit() {
    return Number(this.next() >> 11n) / 2 ** 53;
  }
}
