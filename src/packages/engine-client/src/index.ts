import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { NativeMethod } from "@agent-hud/protocol";

/** Node transport adapter. All collection and decisions stay in the Rust runtime. */
export class EngineClient {
  private child?: ChildProcessWithoutNullStreams;
  private sequence = 0;
  private buffer = "";
  private pending = new Map<
    number,
    {
      resolve: (value: unknown) => void;
      reject: (reason: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  private binary: string;
  private mode: "standalone" | "app";
  constructor(
    binary = process.env.AGENTHUD_ENGINE ??
      fileURLToPath(
        new URL(
          `../../../../target/debug/agent-hud-engine${process.platform === "win32" ? ".exe" : ""}`,
          import.meta.url,
        ),
      ),
    mode: "standalone" | "app" = "standalone",
  ) {
    this.binary = binary;
    this.mode = mode;
  }
  private fail(message: string) {
    for (const request of this.pending.values()) {
      clearTimeout(request.timer);
      request.reject(new Error(message));
    }
    this.pending.clear();
  }
  private start() {
    if (this.child) return this.child;
    if (!existsSync(this.binary))
      throw new Error(
        "Build the Rust engine first: npm run engine:build. Live data is never replaced with demo data.",
      );
    const child = spawn(
      this.binary,
      this.mode === "app" ? ["--app-channel"] : [],
      { stdio: ["pipe", "pipe", "pipe"] },
    );
    this.child = child;
    this.buffer = "";
    child.stderr.on("data", () => {
      /* Engine output is never mixed into MCP stdout or UI responses. */
    });
    child.stdout.on("data", (chunk: Buffer) => {
      this.buffer += chunk.toString("utf8");
      if (this.buffer.length > 16 * 1024 * 1024) {
        this.fail("Engine response exceeded 16 MiB.");
        this.close();
        return;
      }
      let newline: number;
      while ((newline = this.buffer.indexOf("\n")) !== -1) {
        const line = this.buffer.slice(0, newline);
        this.buffer = this.buffer.slice(newline + 1);
        try {
          const response = JSON.parse(line);
          const request = this.pending.get(response.id);
          if (!request) continue;
          clearTimeout(request.timer);
          this.pending.delete(response.id);
          if (response.error)
            request.reject(
              new Error(response.error.message ?? "Engine command failed."),
            );
          else request.resolve(response.result);
        } catch {
          this.fail("Engine returned an invalid protocol frame.");
          this.close();
        }
      }
    });
    child.on("error", (error) => this.fail(error.message));
    child.on("close", () => {
      if (this.child === child) {
        this.child = undefined;
        this.fail("Engine stopped. The last operation was not retried.");
      }
    });
    return child;
  }
  async request<T = unknown>(
    method: NativeMethod | string,
    params: Record<string, unknown> = {},
  ): Promise<T> {
    const child = this.start();
    const id = ++this.sequence;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.fail(
          "Engine request timed out. The last operation was not retried.",
        );
        this.close();
      }, 45000);
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
        timer,
      });
      child.stdin.write(
        JSON.stringify({ id, method, params }) + "\n",
        (error) => {
          if (error) {
            this.fail(error.message);
            this.close();
          }
        },
      );
    });
  }
  close() {
    this.fail("Engine client closed.");
    this.child?.kill();
    this.child = undefined;
  }
}
