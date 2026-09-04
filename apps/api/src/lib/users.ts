import { eq } from "drizzle-orm";
import { db } from "../db";
import { users } from "../db/schema";

export async function upsertUserByClerkId(clerkUserId: string) {
  const existing = await db
    .select()
    .from(users)
    .where(eq(users.clerkUserId, clerkUserId))
    .limit(1);

  const found = existing[0];
  if (found) {
    return found;
  }

  const inserted = await db
    .insert(users)
    .values({ clerkUserId })
    .onConflictDoNothing({ target: users.clerkUserId })
    .returning();

  const created = inserted[0];
  if (created) {
    return created;
  }

  const raced = await db
    .select()
    .from(users)
    .where(eq(users.clerkUserId, clerkUserId))
    .limit(1);
  const user = raced[0];
  if (!user) {
    throw new Error("Failed to upsert user");
  }
  return user;
}
