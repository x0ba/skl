const LOCAL_TOKEN_KEY = "skl.bearerToken";

const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) {
    listener();
  }
}

export function subscribeLocalToken(onStoreChange: () => void): () => void {
  listeners.add(onStoreChange);
  return () => {
    listeners.delete(onStoreChange);
  };
}

export function readLocalToken(): string {
  if (typeof window === "undefined") {
    return "";
  }
  return window.sessionStorage.getItem(LOCAL_TOKEN_KEY) ?? "";
}

export function writeLocalToken(token: string) {
  window.sessionStorage.setItem(LOCAL_TOKEN_KEY, token);
  emit();
}
