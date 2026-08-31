import { mutation, query } from './_generated/server';
import { v } from 'convex/values';

/** Live open-proposals counter — UI subscribes; never polls Hub for this badge. */
export const countOpen = query({
  args: {},
  handler: async (ctx) => {
    const open = await ctx.db
      .query('proposals')
      .withIndex('by_status', (q) => q.eq('status', 'open'))
      .collect();
    return open.length;
  },
});

export const upsert = mutation({
  args: {
    hubId: v.string(),
    status: v.string(),
    projectId: v.optional(v.string()),
    title: v.optional(v.string()),
    updatedAt: v.string(),
  },
  handler: async (ctx, args) => {
    const existing = await ctx.db
      .query('proposals')
      .withIndex('by_hubId', (q) => q.eq('hubId', args.hubId))
      .unique();

    if (existing) {
      await ctx.db.patch(existing._id, {
        status: args.status,
        projectId: args.projectId,
        title: args.title,
        updatedAt: args.updatedAt,
      });
      return existing._id;
    }

    return await ctx.db.insert('proposals', args);
  },
});

export const remove = mutation({
  args: { hubId: v.string() },
  handler: async (ctx, args) => {
    const existing = await ctx.db
      .query('proposals')
      .withIndex('by_hubId', (q) => q.eq('hubId', args.hubId))
      .unique();
    if (existing) {
      await ctx.db.delete(existing._id);
    }
  },
});
