import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { BotAppearanceMenu } from "./BotAppearanceMenu";

it("keeps bot actions behind the edit icon and supports keyboard selection and dismissal", async () => {
  const user = userEvent.setup(), shuffle = vi.fn(), customize = vi.fn();
  render(<BotAppearanceMenu seed="test" disabled={false} shuffle={shuffle} customize={customize} />);
  expect(screen.queryByRole("menu")).toBeNull();
  const edit = screen.getByRole("button", { name: "Edit character appearance" });
  await user.click(edit);
  expect(screen.getByRole("menuitem", { name: "Try another character" })).toHaveFocus();
  await user.keyboard("{ArrowDown}{Enter}");
  expect(customize).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).toBeNull();
  expect(edit).toHaveFocus();
  await user.click(edit);
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).toBeNull();
  expect(edit).toHaveFocus();
  expect(shuffle).not.toHaveBeenCalled();
});
