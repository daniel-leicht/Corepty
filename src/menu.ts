// A lightweight floating context menu, positioned at a point and dismissed on
// outside click / Escape.
import { icon } from "./icons";
import { escapeHtml } from "./util";

export type MenuItem =
  | "sep"
  | { label: string; icon?: string; action: () => void; danger?: boolean };

export function contextMenu(x: number, y: number, items: MenuItem[]): void {
  document.querySelectorAll(".ctx-menu").forEach((m) => m.remove());
  const menu = document.createElement("div");
  menu.className = "ctx-menu pop";
  menu.innerHTML = items
    .map((it) =>
      it === "sep"
        ? '<div class="pop__sep"></div>'
        : `<button class="ctx-item ${it.danger ? "danger" : ""}">${
            it.icon ? icon(it.icon) : ""
          }<span>${escapeHtml(it.label)}</span></button>`
    )
    .join("");
  document.body.appendChild(menu);
  const rect = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(x, window.innerWidth - rect.width - 8)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - rect.height - 8)}px`;

  const buttons = Array.from(menu.querySelectorAll<HTMLButtonElement>(".ctx-item"));
  let bi = 0;
  for (const it of items) {
    if (it === "sep") continue;
    buttons[bi++].addEventListener("click", () => {
      close();
      it.action();
    });
  }
  const close = () => {
    menu.remove();
    document.removeEventListener("mousedown", onOut, true);
    document.removeEventListener("keydown", onKey, true);
  };
  const onOut = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) close();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };
  setTimeout(() => {
    document.addEventListener("mousedown", onOut, true);
    document.addEventListener("keydown", onKey, true);
  });
}
