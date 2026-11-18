import { legacy as renamed, kept } from "@ripplemap/packaged";
import { retained as otherPackage } from "@ripplemap/library";

export const result = renamed(1) + kept(2) + otherPackage(3);

function legacy(value: number): number { return value - 1; }
export const unrelated = legacy(4);
