import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { AppProviders, useSession } from "./providers";

function TokenInput() {
  const session = useSession();
  return <input aria-label="token" value={session.localToken} onChange={e => session.setLocalToken(e.target.value)} />;
}
afterEach(() => { cleanup(); localStorage.clear(); });

it("keeps credential inputs mounted and focused when the cache scope changes", () => {
  const { getByRole } = render(<AppProviders clerkEnabled={false}><TokenInput /></AppProviders>);
  const input = getByRole("textbox");
  act(() => input.focus());
  fireEvent.change(input, { target: { value: "dev:alice" } });
  expect(getByRole("textbox")).toBe(input);
  expect(document.activeElement).toBe(input);
  fireEvent.change(input, { target: { value: "dev:bob" } });
  expect(document.activeElement).toBe(input);
});
