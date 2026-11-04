// The app's engine, and its answers as a tool's result.
import { EngineClient } from "@agent-hud/engine-client";

export const engine = new EngineClient(undefined, "app");

/** A panel tree node with its id and type first and its children last, so the tree
 * reads top down; the engine's JSON comes with its keys sorted. */
const readable = (node: unknown): unknown => {
  if (Array.isArray(node)) return node.map(readable);
  if (typeof node !== "object" || node === null) return node;
  const { id, type, children, ...rest } = node as Record<string, unknown>;
  return {
    ...(id === undefined ? {} : { id }),
    ...(type === undefined ? {} : { type }),
    ...Object.fromEntries(
      Object.entries(rest).map(([key, value]) => [key, readable(value)]),
    ),
    ...(children === undefined ? {} : { children: readable(children) }),
  };
};
const result = (data: unknown) => {
  const object =
    typeof data === "object" && data !== null
      ? (data as Record<string, unknown>)
      : { value: data };
  const { images, ...fields } = object;
  if (fields.panel !== undefined) fields.panel = readable(fields.panel);
  const content: (
    | { type: "text"; text: string }
    | { type: "image"; data: string; mimeType: string }
  )[] = [{ type: "text", text: JSON.stringify(fields) }];
  if (Array.isArray(images))
    for (const image of images)
      if (
        image &&
        typeof image.data === "string" &&
        typeof image.mimeType === "string"
      )
        content.push({
          type: "image",
          data: image.data,
          mimeType: image.mimeType,
        });
  return { content, structuredContent: fields };
};
/** The app's answer to `method`, through `shape`, or the app's error as the tool's. */
export const guarded = async <T>(
  method: string,
  params: Record<string, unknown> = {},
  shape: (answer: T) => unknown = (answer) => answer,
) => {
  try {
    return result(shape(await engine.request<T>(method, params)));
  } catch (error) {
    return {
      isError: true,
      content: [
        {
          type: "text" as const,
          text:
            error instanceof Error ? error.message : "Engine operation failed.",
        },
      ],
    };
  }
};
