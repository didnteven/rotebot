import { probe } from "./probe.ts";
await probe("dynamic-data", () => import("data:text/javascript,export default 123"));
