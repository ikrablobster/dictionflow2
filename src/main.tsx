import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./components/Overlay";
import "./styles/global.css";

const params = new URLSearchParams(window.location.search);
const Root = params.get("view") === "overlay" ? Overlay : App;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><Root /></React.StrictMode>
);
