import React from "react";
import ReactDOM from "react-dom/client";
import "@/styles.css";
import "@/i18n";
import { Popup } from "./Popup";

document.documentElement.classList.add("transparent");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Popup />
  </React.StrictMode>,
);
