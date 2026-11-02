import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App";

const host = document.getElementById("root");
if (!host) throw new Error("Root element missing");
createRoot(host).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
