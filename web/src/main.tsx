import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ToastProvider } from "@/components/toast/ToastProvider";
import { useMessageStore } from "@/stores/useMessageStore";
import "./styles.css";
import "./styles/mobile.css";

// Expose stores on window for zukeeper Chrome DevTools extension
(window as unknown as Record<string, unknown>).store = useMessageStore;

const root = document.getElementById("root");
if (!root) throw new Error("Root element not found");

createRoot(root).render(
  <StrictMode>
    <ToastProvider>
      <App />
    </ToastProvider>
  </StrictMode>,
);
