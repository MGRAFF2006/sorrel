import { hidden } from "./excluded";
import { add } from "../shared/math";
import { missing } from "./missing";

export function helper(value: string): string {
  return `${value}:${hidden}:${add(1, 2)}:${missing}`;
}
