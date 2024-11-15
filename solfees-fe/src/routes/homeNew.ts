import { createFileRoute } from "@tanstack/react-router";
import { HomeNew } from "../pages/HomeNew";

export const Route = createFileRoute("/homeNew")({
  component: HomeNew,
});
