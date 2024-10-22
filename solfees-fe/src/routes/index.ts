import { createFileRoute } from "@tanstack/react-router";
import { RootPage } from "../pages/RootPage.tsx";

export const Route = createFileRoute("/")({
	component: RootPage,
});
