import "../../styles/global.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { platform } from "../../lib/platform";
import { Ball } from "./Ball";

document.documentElement.dataset.platform = platform;
createRoot(document.getElementById("root")!).render(<StrictMode><Ball /></StrictMode>);
