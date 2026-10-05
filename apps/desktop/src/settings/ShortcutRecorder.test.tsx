// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ShortcutRecorder } from "@/settings/ShortcutRecorder";

function setup(props: Partial<Parameters<typeof ShortcutRecorder>[0]> = {}) {
  const onChange = vi.fn(async () => null as string | null);
  const onRecording = vi.fn();
  render(
    <ShortcutRecorder
      label="Capture area"
      shortcut="Alt+Shift+Cmd+KeyS"
      onChange={onChange}
      onRecording={onRecording}
      {...props}
    />,
  );
  return { onChange, onRecording, user: userEvent.setup() };
}

const button = () =>
  screen.getByRole("button", { name: "Capture area shortcut" });

describe("ShortcutRecorder", () => {
  it("shows the shortcut the way macOS does", () => {
    setup();
    expect(button().textContent).toBe("⌥⇧⌘S");
  });

  it("records a new shortcut and pauses global shortcuts meanwhile", async () => {
    const { onChange, onRecording, user } = setup();
    await user.click(button());
    expect(onRecording).toHaveBeenLastCalledWith(true);
    expect(button().textContent).toBe("Type shortcut…");

    await user.keyboard("{Control>}{Meta>}");
    expect(button().textContent).toBe("⌃⌘");
    await user.keyboard("[Digit7]{/Meta}{/Control}");

    expect(onChange).toHaveBeenCalledWith("Ctrl+Cmd+Digit7");
    expect(onRecording).toHaveBeenLastCalledWith(false);
  });

  it("cancels with Esc and turns off with Delete", async () => {
    const { onChange, user } = setup();
    await user.click(button());
    await user.keyboard("{Escape}");
    expect(onChange).not.toHaveBeenCalled();
    expect(button().textContent).toBe("⌥⇧⌘S");

    await user.click(button());
    await user.keyboard("{Backspace}");
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it("shows conflicts reported when saving", async () => {
    const { user } = setup({
      onChange: async () =>
        "⇧⌘4 is already used by the macOS screenshot of an area.",
    });
    await user.click(button());
    await user.keyboard("{Shift>}{Meta>}[Digit4]{/Meta}{/Shift}");
    expect((await screen.findByRole("alert")).textContent).toMatch(
      /already used by the macOS screenshot/,
    );
  });

  it("shows registration failures and offers to turn the shortcut off", async () => {
    const { onChange, user } = setup({ error: "macOS didn't accept ⌥⇧⌘S" });
    expect(screen.getByRole("alert").textContent).toBe(
      "macOS didn't accept ⌥⇧⌘S",
    );
    await user.click(
      screen.getByRole("button", { name: "Turn off Capture area shortcut" }),
    );
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it("says when a key can't be used", async () => {
    const { onChange, user } = setup({ shortcut: null });
    expect(button().textContent).toBe("Record Shortcut");
    await user.click(button());
    await user.keyboard("{Meta>}{Escape}{/Meta}");
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toMatch(
      /Escape can't be used/,
    );
  });
});
