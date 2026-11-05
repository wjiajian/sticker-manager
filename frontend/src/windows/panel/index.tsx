import "../../styles/global.css";
import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { platform } from "../../lib/platform";
import { queryClient } from "../../lib/queries";
import { Panel } from "./Panel";

document.documentElement.dataset.platform = platform;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Panel />
    </QueryClientProvider>
  </StrictMode>,
);
