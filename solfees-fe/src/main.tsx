import { createMemoryHistory, createRouter } from "@tanstack/react-router";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App.tsx";
import { routeTree } from "./routeTree.gen.ts";
import "./styles/tailwind.css";
import "./styles/global-styles.css";
import "./common/i18n";

const memoryHistory = createMemoryHistory({
  initialEntries: ["/"],
});

const router = createRouter({ routeTree, history: memoryHistory });

declare module "@tanstack/react-router" {
  interface Register {
    // This infers the type of our router and registers it across your entire project
    router: typeof router;
  }
}

const rootElement = document.querySelector("#root") as Element;
if (!rootElement.innerHTML) {
  const root = ReactDOM.createRoot(rootElement);
  root.render(
    <React.Suspense fallback="loading">
      <App router={router} />
    </React.Suspense>
  );
}
