import { createFileRoute } from "@tanstack/react-router";
import { HomeOld } from "../pages/HomeOld";

export const Route = createFileRoute("/homeOld")({
	component: HomeOld,
});
