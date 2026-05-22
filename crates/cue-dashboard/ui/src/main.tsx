import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { installFrontendErrorHandlers } from "./lib/tauri";
import "./index.css";

installFrontendErrorHandlers();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
