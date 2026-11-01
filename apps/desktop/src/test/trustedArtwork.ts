import { expect } from "vitest";

/** Validate the local bot subtree before excluding it from injection assertions. */
export function withoutBotArtwork(root: HTMLElement): HTMLElement {
  const copy = root.cloneNode(true) as HTMLElement;
  for (const bot of copy.querySelectorAll(".bot-avatar")) {
    expect(bot.tagName).toBe("SPAN");
    expect(bot.getAttribute("aria-hidden")).toBe("true");
    expect(bot.getAttribute("style")).toMatch(/^width: \d+px; height: \d+px;$/);
    expect(bot.children).toHaveLength(1);
    const sprite = bot.firstElementChild!;
    if (sprite.tagName === "IMG") {
      expect(sprite.getAttributeNames().sort()).toEqual(["alt", "class", "draggable", "src"]);
      expect(sprite.className).toBe("bot-custom-image");
      expect(sprite.getAttribute("alt")).toBe("");
      expect(sprite.getAttribute("draggable")).toBe("false");
      expect(sprite.getAttribute("src")).toMatch(/^data:image\/png;base64,iVBORw0KGgo[A-Za-z0-9+/]*={0,2}$/);
    } else {
      expect(sprite.tagName).toBe("SPAN");
      expect(sprite.getAttributeNames()).toEqual(["class"]);
      expect(sprite.className).toMatch(/^bot-sprite bot-family-[0-3] bot-pose-[0-5] bot-tint-[0-5]$/);
      expect(sprite.children).toHaveLength(0);
      expect(sprite.textContent).toBe("");
    }
    bot.remove();
  }
  return copy;
}
