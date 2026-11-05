import React from "react";
import ReactDOM from "react-dom/client";
import "@/styles.css";
import "@/i18n";
import { Popup } from "./Popup";

document.documentElement.classList.add("transparent");

function render() {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Popup />
    </React.StrictMode>,
  );
}

// `?mock` on the dev server renders with sample data instead of the Rust backend.
if (import.meta.env.DEV && new URLSearchParams(location.search).has("mock")) {
  void import("@/dev/mock").then((m) => {
    m.install("popup");
    render();
  });
} else {
  render();
}
