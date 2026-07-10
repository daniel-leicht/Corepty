import "./styles.css";
import "@xterm/xterm/css/xterm.css";
import { App } from "./app";

const root = document.querySelector<HTMLDivElement>("#app")!;
const app = new App(root);
void app.mount();
