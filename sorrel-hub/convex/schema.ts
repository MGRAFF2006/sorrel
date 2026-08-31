import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';

/**
 * Shared Convex metadata schema for SaaS (Convex Cloud) and self-host.
 * VCS objects/refs stay on the Hub sync object store — never here.
 */
export default defineSchema({
  proposals: defineTable({
    hubId: v.string(),
    status: v.string(),
    projectId: v.optional(v.string()),
    title: v.optional(v.string()),
    updatedAt: v.string(),
  })
    .index('by_hubId', ['hubId'])
    .index('by_status', ['status']),
});
