// Settings modal. Changes apply live and persist immediately.
import { icon } from "./icons";
import {
  ACCENT_PRESETS,
  current,
  DEFAULTS,
  persistSettings,
  type Settings,
} from "./settings";

export class SettingsDialog {
  private el: HTMLElement | null = null;
  constructor(private onChange: () => void) {}

  open(): void {
    this.close();
    const backdrop = document.createElement("div");
    backdrop.className = "modal-backdrop";
    backdrop.innerHTML = this.template();
    document.body.appendChild(backdrop);
    this.el = backdrop;
    this.wire(backdrop);
    backdrop.addEventListener("mousedown", (e) => {
      if (e.target === backdrop) this.close();
    });
    document.addEventListener("keydown", this.escHandler, true);
  }

  private escHandler = (e: KeyboardEvent) => {
    if (e.key === "Escape") this.close();
  };

  private close(): void {
    this.el?.remove();
    this.el = null;
    document.removeEventListener("keydown", this.escHandler, true);
  }

  private template(): string {
    const s = current;
    return `
      <div class="modal modal--wide" role="dialog" aria-modal="true">
        <div class="modal__head">
          <div class="modal__title">${icon("settings")} Settings</div>
          <button class="modal__close" data-act="close">${icon("close")}</button>
        </div>
        <div class="modal__body">
          <div class="settings-section">Appearance</div>

          <div class="field">
            <span class="field__label">Accent</span>
            <div class="swatches">
              ${ACCENT_PRESETS.map(
                (p) =>
                  `<button type="button" class="swatch ${p.accent === s.accent ? "is-active" : ""}" data-accent="${p.accent}" data-accent2="${p.accent2}" style="background:linear-gradient(135deg, ${p.accent}, ${p.accent2})" title="${p.name}"></button>`
              ).join("")}
              <label class="swatch swatch-custom" title="Custom accent">
                ${icon("pencil")}
                <input type="color" data-role="accent" value="${s.accent}" />
              </label>
              <label class="swatch swatch-custom" title="Custom gradient end">
                ${icon("pencil")}
                <input type="color" data-role="accent2" value="${s.accent2}" />
              </label>
            </div>
          </div>

          <label class="field">
            <span class="field__label">Font family</span>
            <input data-role="fontFamily" value="${escAttr(s.fontFamily)}" spellcheck="false" />
          </label>

          <div class="grid2">
            <label class="field">
              <span class="field__label">Font size <span data-fslabel>${s.fontSize}px</span></span>
              <input type="range" min="9" max="24" step="0.5" data-role="fontSize" value="${s.fontSize}" />
            </label>
            <label class="field">
              <span class="field__label">Line height <span data-lhlabel>${s.lineHeight}</span></span>
              <input type="range" min="1" max="2" step="0.05" data-role="lineHeight" value="${s.lineHeight}" />
            </label>
          </div>

          <div class="grid2">
            <div class="field">
              <span class="field__label">Cursor</span>
              <div class="seg" data-role="cursorStyle">${seg(["bar", "block", "underline"], s.cursorStyle)}</div>
            </div>
            <label class="check cursor-blink">
              <input type="checkbox" data-role="cursorBlink" ${s.cursorBlink ? "checked" : ""} />
              <span>Blink cursor</span>
            </label>
          </div>

          <div class="settings-section">Terminal</div>

          <div class="grid2">
            <label class="field">
              <span class="field__label">Scrollback (lines)</span>
              <input type="number" min="100" max="200000" step="100" data-role="scrollback" value="${s.scrollback}" />
            </label>
            <label class="field">
              <span class="field__label">Default shell</span>
              <select data-role="defaultShell" class="select">
                ${shellOpts(s.defaultShell)}
              </select>
            </label>
          </div>

          <div class="grid2">
            <div class="field">
              <span class="field__label">Bell</span>
              <div class="seg" data-role="bell">${seg(["none", "visual", "sound"], s.bell)}</div>
            </div>
            <div class="field">
              <span class="field__label">Right-click</span>
              <div class="seg" data-role="rightClick">${seg(["paste", "menu"], s.rightClick)}</div>
            </div>
          </div>

          <label class="check">
            <input type="checkbox" data-role="copyOnSelect" ${s.copyOnSelect ? "checked" : ""} />
            <span>Copy on selection (PuTTY-style)</span>
          </label>
        </div>
        <div class="modal__foot">
          <button type="button" class="btn ghost" data-act="reset">Reset to defaults</button>
          <div class="spacer"></div>
          <button type="button" class="btn primary" data-act="close">Done</button>
        </div>
      </div>`;
  }

  private wire(root: HTMLElement): void {
    const apply = () => {
      void persistSettings();
      this.onChange();
    };
    const set = <K extends keyof Settings>(k: K, v: Settings[K]) => {
      current[k] = v;
      apply();
    };

    // text / range / number / select / checkbox controls
    root.querySelectorAll<HTMLElement>("[data-role]").forEach((elm) => {
      const role = elm.dataset.role as keyof Settings;
      if (elm instanceof HTMLInputElement && elm.type === "color") {
        elm.addEventListener("input", () => set(role, elm.value as Settings[typeof role]));
      } else if (elm instanceof HTMLInputElement && elm.type === "range") {
        elm.addEventListener("input", () => {
          const num = parseFloat(elm.value);
          set(role, num as Settings[typeof role]);
          if (role === "fontSize") root.querySelector("[data-fslabel]")!.textContent = `${num}px`;
          if (role === "lineHeight") root.querySelector("[data-lhlabel]")!.textContent = `${num}`;
        });
      } else if (elm instanceof HTMLInputElement && elm.type === "number") {
        elm.addEventListener("change", () => set(role, (parseInt(elm.value, 10) || 100) as Settings[typeof role]));
      } else if (elm instanceof HTMLInputElement && elm.type === "checkbox") {
        elm.addEventListener("change", () => set(role, elm.checked as Settings[typeof role]));
      } else if (elm instanceof HTMLInputElement || elm instanceof HTMLSelectElement) {
        elm.addEventListener("change", () => set(role, elm.value as Settings[typeof role]));
      } else if (elm.classList.contains("seg")) {
        elm.querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
          btn.addEventListener("click", () => {
            elm.querySelectorAll("button").forEach((b) => b.classList.remove("is-active"));
            btn.classList.add("is-active");
            set(role, btn.dataset.val as Settings[typeof role]);
          });
        });
      }
    });

    // accent preset swatches
    root.querySelectorAll<HTMLButtonElement>(".swatch[data-accent]").forEach((sw) => {
      sw.addEventListener("click", () => {
        root.querySelectorAll(".swatch").forEach((x) => x.classList.remove("is-active"));
        sw.classList.add("is-active");
        current.accent = sw.dataset.accent!;
        current.accent2 = sw.dataset.accent2!;
        const a = root.querySelector<HTMLInputElement>('input[data-role="accent"]');
        const a2 = root.querySelector<HTMLInputElement>('input[data-role="accent2"]');
        if (a) a.value = current.accent;
        if (a2) a2.value = current.accent2;
        apply();
      });
    });

    root.querySelectorAll<HTMLElement>('[data-act="close"]').forEach((b) =>
      b.addEventListener("click", () => this.close())
    );
    root.querySelector('[data-act="reset"]')?.addEventListener("click", () => {
      Object.assign(current, DEFAULTS);
      apply();
      // rebuild the modal to reflect defaults
      this.open();
    });
  }
}

function seg(values: string[], active: string): string {
  return values
    .map(
      (v) =>
        `<button type="button" data-val="${v}" class="${v === active ? "is-active" : ""}">${cap(v)}</button>`
    )
    .join("");
}

function shellOpts(active: string): string {
  const opts: Array<[string, string]> = [
    ["powershell", "PowerShell"],
    ["pwsh", "PowerShell 7"],
    ["cmd", "Command Prompt"],
    ["bash", "Bash"],
  ];
  return opts
    .map(([v, label]) => `<option value="${v}" ${v === active ? "selected" : ""}>${label}</option>`)
    .join("");
}

function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function escAttr(s: string): string {
  return s.replace(/[&<>"]/g, (c) =>
    c === "&" ? "&amp;" : c === "<" ? "&lt;" : c === ">" ? "&gt;" : "&quot;"
  );
}
