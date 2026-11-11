import { removed as renamed } from "@ripplemap/library";

export const result = renamed(1);

function removed(value: number): number {
  return value - 1;
}
export const unrelated = removed(2);
