import "../../styles/global.css";
import { createRoot } from "react-dom/client";
import { Preview } from "./Preview";

document.documentElement.dataset.window = "preview";
createRoot(document.getElementById("root")!).render(<Preview />);
