/** A working folder's last part, as a request or a session names its project. */
export function projectName(cwd?: string | null): string | null {
  if (!cwd) return null;
  const trimmed = cwd.replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).pop();
  return name && name.length > 0 ? name : null;
}

/** A path with the home folder written as ~. */
export function shortenHomePath(path?: string | null): string | null {
  if (!path) return null;
  return path.replace(/^\/Users\/[^/]+/, "~").replace(/^\/home\/[^/]+/, "~");
}
