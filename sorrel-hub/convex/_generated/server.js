/* eslint-disable */
/**
 * Minimal hand stubs for Convex function builders before codegen.
 * `npx convex dev` regenerates these against Cloud or self-hosted Convex.
 */

type QueryCtx = { db: any };
type MutationCtx = { db: any };

export function query<Args extends Record<string, unknown>, Output>(def: {
  args: Args | Record<string, unknown>;
  handler: (ctx: QueryCtx, args: any) => Promise<Output> | Output;
}): any {
  return def;
}

export function mutation<Args extends Record<string, unknown>, Output>(def: {
  args: Args | Record<string, unknown>;
  handler: (ctx: MutationCtx, args: any) => Promise<Output> | Output;
}): any {
  return def;
}

export function action(def: any): any {
  return def;
}

export function internalQuery(def: any): any {
  return def;
}

export function internalMutation(def: any): any {
  return def;
}

export function internalAction(def: any): any {
  return def;
}
